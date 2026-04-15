use alloc::{format, sync::Arc, vec::Vec};
use core::ffi::CStr;

use basic::{constants::io::OpenFlags, println, println_color, sync::Mutex};
use interface::*;
use shared_heap::{DBox, DVec};
use spin::{Lazy, Once};
use vfs_common::meta::KernelFileMeta;
use vfscore::{dentry::VfsDentry, path::VfsPath, VfsResult};

use crate::{
    devfs, insert_dentry, kfile::KernelFile, pipefs, procfs, ramfs::init_ramfs,
    shim::RootShimDentry, sys, VFS_MAP, VFS_MAP_SHADOW,
};

static SYSTEM_ROOT_FS: Once<Arc<dyn VfsDentry>> = Once::new();

fn common_load_or_create_fs(
    create: bool,
    name: &str,
    mp: &[u8],
    is_init_done: bool,
) -> Arc<dyn VfsDentry> {
    let (fs_domain, fs_ident) = if create {
        let mut ramfs_ident = [0u8; 32];
        let domain = basic::create_domain(name, ramfs_ident.as_mut_slice()).unwrap();
        let name = CStr::from_bytes_until_nul(ramfs_ident.as_ref())
            .unwrap()
            .to_str()
            .unwrap();
        let name = Arc::new(Vec::from(name));
        (domain, name)
    } else {
        (basic::get_domain(name).unwrap(), Arc::new(Vec::from(name)))
    };

    (match fs_domain {
        DomainType::FsDomain(fs) => {
            if !is_init_done {
                let mp = DVec::from_slice(mp);
                let root_inode_id = fs.mount(&mp, None).unwrap();
                RootShimDentry::new(fs, root_inode_id, fs_ident)
            } else {
                let root_inode_id = fs.root_inode_id().unwrap();
                RootShimDentry::new(fs, root_inode_id, fs_ident)
            }
        }
        _ => panic!("{} domain not found", name),
    }) as _
}

fn init_filesystem_before(initrd: &[u8]) -> VfsResult<Arc<dyn VfsDentry>> {
    let ramfs_root = common_load_or_create_fs(false, "ramfs-1", b"/", false);
    init_ramfs(&ramfs_root)?;
    SYSTEM_ROOT_FS.call_once(|| ramfs_root.clone());

    let procfs_root = common_load_or_create_fs(false, "procfs", b"/proc", false);
    procfs::init_procfs(&procfs_root);

    let devfs_domain = basic::get_domain("devfs").unwrap();
    let devfs_root = match devfs_domain {
        DomainType::DevFsDomain(devfs) => {
            let mp = DVec::from_slice(b"/dev");
            let root_inode_id = devfs.mount(&mp, None).unwrap();
            let shim_root_dentry: Arc<dyn VfsDentry> =
                RootShimDentry::new(devfs.clone(), root_inode_id, Arc::new(Vec::from("devfs")));
            devfs::init_devfs(&devfs, &shim_root_dentry);
            shim_root_dentry
        }
        _ => panic!("devfs domain not found"),
    };

    let sysfs_root = common_load_or_create_fs(false, "sysfs", b"/sys", false);
    sys::init_sysfs(&sysfs_root);
    let tmpfs_root = common_load_or_create_fs(true, "ramfs", b"/tmp", false);
    let pipefs_root = common_load_or_create_fs(false, "pipefs", b"/pipe", false);
    pipefs::init_pipefs(&pipefs_root);

    let shm_ramfs_root = common_load_or_create_fs(true, "ramfs", b"/dev/shm", false);
    let domain_fs_root = common_load_or_create_fs(false, "domainfs", b"/domain", false);
    let path = VfsPath::new(ramfs_root.clone(), ramfs_root.clone());
    path.join("proc")?.mount(procfs_root, 0)?;
    path.join("sys")?.mount(sysfs_root, 0)?;
    path.join("dev")?.mount(devfs_root, 0)?;
    path.join("tmp")?.mount(tmpfs_root.clone(), 0)?;
    path.join("dev/shm")?.mount(shm_ramfs_root, 0)?;
    path.join("domain")?.mount(domain_fs_root, 0)?;

    crate::initrd::populate_initrd(ramfs_root.clone(), initrd)?;

    {
        let mut map = VFS_MAP.write();
        let mut map_shadow = VFS_MAP_SHADOW.lock();
        map.insert(
            VFS_ROOT_ID,
            Arc::new(KernelFile::new(
                ramfs_root.clone(),
                OpenFlags::O_RDWR,
                VFS_ROOT_ID,
            )),
        );
        map.insert(VFS_STDIN_ID, STDIN.clone());
        map.insert(VFS_STDOUT_ID, STDOUT.clone());
        map.insert(VFS_STDERR_ID, STDERR.clone());
        map_shadow.insert(VFS_ROOT_ID, ());
        map_shadow.insert(VFS_STDIN_ID, ());
        map_shadow.insert(VFS_STDOUT_ID, ());
        map_shadow.insert(VFS_STDERR_ID, ());
    }

    match basic::get_domain("fatfs-1") {
        Some(DomainType::FsDomain(fatfs)) => {
            println!("fatfs mount begin: /tests");
            let blk_inode = match path.join("/dev/sda") {
                Ok(node) => match node.open(None) {
                    Ok(inode) => inode,
                    Err(e) => {
                        println_color!(93, "fatfs skip: open /dev/sda failed: {:?}", e);
                        return Ok(ramfs_root);
                    }
                },
                Err(e) => {
                    println_color!(93, "fatfs skip: lookup /dev/sda failed: {:?}", e);
                    return Ok(ramfs_root);
                }
            };
            println!("fatfs 已打开块设备 /dev/sda");
            let _id = insert_dentry(blk_inode.clone(), OpenFlags::O_RDWR);
            let mp = DVec::from_slice(b"/tests");
            let blk_inode = match blk_inode.downcast_arc::<RootShimDentry>() {
                Ok(inode) => inode,
                Err(_) => {
                    println_color!(93, "fatfs skip: blk_inode downcast failed");
                    return Ok(ramfs_root);
                }
            };
            let domain_ident = blk_inode.fs_domain_ident_str();
            let mount_inode_id = blk_inode.inode_id();
            let info = MountInfo {
                mount_inode_id,
                domain_ident: {
                    let mut ident = [0; 32];
                    let min_copy = core::cmp::min(ident.len(), domain_ident.len());
                    ident[..min_copy].copy_from_slice(&domain_ident.as_bytes()[..min_copy]);
                    ident
                },
            };
            let root_inode_id = match fatfs.mount(&mp, Some(DBox::new(info))) {
                Ok(id) => id,
                Err(e) => {
                    println_color!(93, "fatfs skip: mount /tests failed: {:?}", e);
                    return Ok(ramfs_root);
                }
            };
            let shim_inode =
                RootShimDentry::new(fatfs, root_inode_id, Arc::new(Vec::from("fatfs-1")));
            match path.join("tests") {
                Ok(node) => {
                    if let Err(e) = node.mount(shim_inode, 0) {
                        println_color!(93, "fatfs skip: mountpoint /tests attach failed: {:?}", e);
                        return Ok(ramfs_root);
                    }
                }
                Err(e) => {
                    println_color!(93, "fatfs skip: create /tests mountpoint failed: {:?}", e);
                    return Ok(ramfs_root);
                }
            }
            println!("fatfs mount success: /tests");
        }
        Some(_) => {
            println_color!(93, "fatfs skip: fatfs-1 domain type mismatch");
        }
        None => {
            println_color!(93, "fatfs skip: fatfs-1 domain not found");
        }
    }
    Ok(ramfs_root)
}

fn init_filesystem_after() -> VfsResult<Arc<dyn VfsDentry>> {
    // println_color!(31, "Init filesystem after");
    let ramfs_root = common_load_or_create_fs(false, "ramfs-1", b"/", true);
    SYSTEM_ROOT_FS.call_once(|| ramfs_root.clone());
    let pipefs_root = common_load_or_create_fs(false, "pipefs", b"", true);
    pipefs::init_pipefs(&pipefs_root);
    {
        // recover the file descriptor
        // println!("recover the file descriptor");
        let mut map = VFS_MAP.write();
        let map_shadow = VFS_MAP_SHADOW.lock();
        for (id, _) in map_shadow.iter() {
            let key = format!("kfile_{}", id);
            let meta = storage::get::<Mutex<KernelFileMeta>>(&key).unwrap();
            let real_inode_id = meta.lock().real_inode_id;
            let fs_domain = meta.lock().fs_domain.clone();
            let dentry = {
                let fs_domain_ident = &meta.lock().fs_domain_ident;
                let _ident = core::str::from_utf8(fs_domain_ident.as_slice()).unwrap();
                // println!("recover file descriptor: {} in fs domain {}", key, ident);
                RootShimDentry::new(
                    fs_domain,
                    real_inode_id,
                    Arc::new(Vec::from(fs_domain_ident.as_slice())),
                )
            };
            let kfile = Arc::new(KernelFile::from_meta(dentry, meta, *id));
            map.insert(*id, kfile);
        }
    }
    Ok(ramfs_root)
}

/// Init the filesystem
pub fn init_filesystem(initrd: &[u8], is_init_done: bool) -> VfsResult<()> {
    let _ramfs_root = if is_init_done {
        init_filesystem_after()?
    } else {
        init_filesystem_before(initrd)?
    };
    // println!("Vfs Tree:");
    // vfscore::path::print_fs_tree(&mut VfsOutPut, ramfs_root, "".to_string(), false).unwrap();
    // println!("Init filesystem success");
    Ok(())
}

/// Get the root filesystem of the system
#[inline]
pub fn system_root_fs() -> Arc<dyn VfsDentry> {
    SYSTEM_ROOT_FS.get().unwrap().clone()
}

type Stdin = KernelFile;
type Stdout = KernelFile;

pub static STDIN: Lazy<Arc<Stdin>> = Lazy::new(|| {
    let path = VfsPath::new(system_root_fs(), system_root_fs())
        .join("dev/tty")
        .unwrap();
    let dentry = path.open(None).unwrap();
    let file = KernelFile::new(dentry, OpenFlags::O_RDONLY, VFS_STDIN_ID);
    Arc::new(file)
});

pub static STDOUT: Lazy<Arc<Stdout>> = Lazy::new(|| {
    let path = VfsPath::new(system_root_fs(), system_root_fs())
        .join("dev/tty")
        .unwrap();
    let dentry = path.open(None).unwrap();
    let file = KernelFile::new(dentry, OpenFlags::O_WRONLY, VFS_STDOUT_ID);
    Arc::new(file)
});

pub static STDERR: Lazy<Arc<Stdout>> = Lazy::new(|| {
    let path = VfsPath::new(system_root_fs(), system_root_fs())
        .join("dev/tty")
        .unwrap();
    let dentry = path.open(None).unwrap();
    let file = KernelFile::new(dentry, OpenFlags::O_WRONLY, VFS_STDERR_ID);
    Arc::new(file)
});
