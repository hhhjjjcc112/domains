#![no_std]
#![forbid(unsafe_code)]


extern crate alloc;
use alloc::{
    collections::BTreeMap,
    string::String,
    sync::Arc,
    vec::Vec,
};
use core::{
    ffi::CStr,
    fmt::{Debug, Formatter},
    ops::Index,
    sync::atomic::AtomicU64,
};

use basic::{println, sync::Mutex, *};
use interface::{
    define_unwind_for_FsDomain, Basic, DirEntryWrapper, DomainType, FsDomain, InodeID, MountInfo,
    VfsDomain,
};
use shared_heap::{DBox, DVec};
use spin::Once;
use vfs_common::shim::{FsShimInode, RootShimDentry};
use vfscore::{
    dentry::VfsDentry,
    fstype::{FileSystemFlags, VfsFsType},
    inode::{InodeAttr, VfsInode},
    superblock::SuperType,
    utils::{
        VfsFileStat, VfsFsStat, VfsNodePerm, VfsNodeType, VfsPollEvents, VfsRenameFlag, VfsTime,
        VfsTimeSpec,
    },
};

pub static VFS_DOMAIN: Once<Arc<dyn VfsDomain>> = Once::new();

pub static ROOT_DENTRY: Once<Arc<dyn VfsDentry>> = Once::new();

pub struct GenericFsDomain {
    fs: Arc<dyn VfsFsType>,
    dentry_map: Mutex<BTreeMap<InodeID, Arc<dyn VfsDentry>>>,
    inode_index: AtomicU64,
    name: String,
    mount_func: Option<fn(root: &Arc<dyn VfsDentry>)>,
    init_func: Option<fn()>,
    parent_dentry_map: Mutex<BTreeMap<InodeID, Arc<dyn VfsDentry>>>,
    magic: Once<u128>,
}

impl Debug for GenericFsDomain {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GenericFsDomain")
            .field("name", &self.name)
            .finish()
    }
}

impl GenericFsDomain {
    pub fn new(
        fs: Arc<dyn VfsFsType>,
        name: String,
        mount_func: Option<fn(root: &Arc<dyn VfsDentry>)>,
        init_func: Option<fn()>,
    ) -> Self {
        Self {
            fs,
            dentry_map: Mutex::new(BTreeMap::new()),
            inode_index: AtomicU64::new(0),
            mount_func,
            init_func,
            name,
            parent_dentry_map: Mutex::new(BTreeMap::new()),
            magic:Once::new()
        }
    }

    pub fn root_dentry(&self) -> Arc<dyn VfsDentry> {
        ROOT_DENTRY.get().unwrap().clone()
    }
}

impl Basic for GenericFsDomain {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl FsDomain for GenericFsDomain {
    fn init(&self) -> AlienResult<()> {
        let vfs_domain = basic::get_domain("vfs").unwrap();
        let vfs_domain = match vfs_domain {
            DomainType::VfsDomain(vfs_domain) => vfs_domain,
            _ => panic!("vfs domain not found"),
        };
        VFS_DOMAIN.call_once(|| vfs_domain);
        if let Some(init) = self.init_func {
            init();
        }
        println!("{} FsDomain init", self.name);
        Ok(())
    }
    fn mount(
        &self,
        mount_point: &DVec<u8>,
        dev_inode: Option<DBox<MountInfo>>,
    ) -> AlienResult<InodeID> {
        let mount_point = core::str::from_utf8(mount_point.as_slice()).unwrap();
        let dev_inode: Option<Arc<dyn VfsInode>> = match dev_inode {
            None => None,
            Some(mount_info) => {
                let id = mount_info.mount_inode_id;
                let fs_domain_name = CStr::from_bytes_until_nul(mount_info.domain_ident.as_slice())
                    .unwrap()
                    .to_str()
                    .unwrap();
                let fs_domain = basic::get_domain(fs_domain_name)
                    .unwrap_or_else(|| panic!("{} domain not found", fs_domain_name));
                let fs_domain = match fs_domain {
                    DomainType::FsDomain(fs_domain) => fs_domain,
                    DomainType::DevFsDomain(fs_domain) => fs_domain,
                    _ => panic!("{} domain not found", fs_domain_name),
                };
                let shim_dev_inode =
                    FsShimInode::new(fs_domain, id, Arc::new(Vec::from(fs_domain_name)));
                Some(Arc::new(shim_dev_inode))
            }
        };
        let root = self.fs.i_mount(0, mount_point, dev_inode, &[])?;
        if let Some(func) = self.mount_func {
            func(&root);
        }

        let magic = root.inode()?.get_super_block()?.magic();
        self.magic.call_once(|| magic);

        let inode_id = self
            .inode_index
            .fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        self.dentry_map.lock().insert(inode_id, root.clone());
        ROOT_DENTRY.call_once(|| root);
        assert_eq!(inode_id, 0);
        println!("{} mount success", self.name);
        Ok(inode_id)
    }

    fn root_inode_id(&self) -> AlienResult<InodeID> {
        Ok(0)
    }

    fn drop_inode(&self, inode: InodeID) -> AlienResult<()> {
        if inode != 0 {
            self.dentry_map.lock().remove(&inode);
        }
        Ok(())
    }

    fn dentry_name(
        &self,
        inode: InodeID,
        mut buf: DVec<u8>,
    ) -> AlienResult<(DVec<u8>, usize)> {
        let inode = self
            .dentry_map
            .lock()
            .get(&inode)
            .unwrap_or_else(|| panic!("dentry {} not found in {}", inode, self.name))
            .clone();
        let name = inode.name();
        let copy_len = core::cmp::min(name.len(), buf.len());
        buf.as_mut_slice()[..copy_len].copy_from_slice(&name.as_bytes()[..copy_len]);
        Ok((buf, copy_len))
    }

    fn dentry_path(
        &self,
        inode: InodeID,
        mut buf: DVec<u8>,
    ) -> AlienResult<(DVec<u8>, usize)> {
        let inode = self.dentry_map.lock().index(&inode).clone();
        let path = inode.path();
        let copy_len = core::cmp::min(path.len(), buf.len());
        buf.as_mut_slice()[..copy_len].copy_from_slice(&path.as_bytes()[..copy_len]);
        Ok((buf, copy_len))
    }

    fn dentry_set_parent(
        &self,
        inode: InodeID,
        domain_ident: &DVec<u8>,
        parent: InodeID,
    ) -> AlienResult<()> {
        let dentry = self.dentry_map.lock().index(&inode).clone();
        let fs_domain_name = core::str::from_utf8(domain_ident.as_slice()).unwrap();
        let fs_domain = basic::get_domain(fs_domain_name)
            .unwrap_or_else(|| panic!("{} domain not found", fs_domain_name));
        let fs_domain = match fs_domain {
            DomainType::FsDomain(fs_domain) => fs_domain,
            DomainType::DevFsDomain(fs_domain) => fs_domain,
            _ => panic!("{} domain not found", fs_domain_name),
        };
        let parent_shim_dentry =
            RootShimDentry::new(fs_domain, parent, Arc::new(Vec::from(fs_domain_name)));
        self.parent_dentry_map
            .lock()
            .insert(parent, parent_shim_dentry.clone());
        dentry.set_parent(&(parent_shim_dentry as Arc<dyn VfsDentry>));
        Ok(())
    }

    fn dentry_parent(&self, inode: InodeID) -> AlienResult<Option<InodeID>> {
        let dentry = self.dentry_map.lock().index(&inode).clone();
        let parent = dentry.parent();
        let parent = match parent {
            Some(parent) => parent,
            None => return Ok(None),
        };
        // let dentry_name = &dentry.name();
        // let parent_name = &parent.name();
        // println_color!(31, "{} parent is {}", dentry_name, parent_name);
        let id = self
            .inode_index
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        self.dentry_map.lock().insert(id, parent);
        Ok(Some(id))
    }

    fn dentry_to_mount_point(
        &self,
        inode: InodeID,
        domain_ident: &DVec<u8>,
        mount_inode_id: InodeID,
    ) -> AlienResult<()> {
        let dentry = self.dentry_map.lock().index(&inode).clone();
        let fs_domain_name = core::str::from_utf8(domain_ident.as_slice()).unwrap();
        let fs_domain = basic::get_domain(fs_domain_name).unwrap();
        let fs_domain = match fs_domain {
            DomainType::FsDomain(fs_domain) => fs_domain,
            DomainType::DevFsDomain(fs_domain) => fs_domain,
            _ => panic!("{} domain not found", fs_domain_name),
        };
        // println_color!(31,"<dentry_to_mount_point> mount {} to {}, domain_name: {}", inode, mount_inode_id, fs_domain_name);
        let mount_shim_dentry = RootShimDentry::new(
            fs_domain,
            mount_inode_id,
            Arc::new(Vec::from(fs_domain_name)),
        );
        dentry.to_mount_point(mount_shim_dentry, 0)?;
        Ok(())
    }

    fn dentry_mount_point(
        &self,
        inode: InodeID,
        mut domain_ident: DVec<u8>,
    ) -> AlienResult<Option<(DVec<u8>, InodeID)>> {
        let dentry = self
            .dentry_map
            .lock()
            .get(&inode)
            .unwrap_or_else(|| panic!("dentry {} not found in {}", inode, self.name))
            .clone();
        let mount_point = dentry.mount_point();
        let mount_point = match mount_point {
            Some(mount_point) => mount_point,
            None => return Ok(None),
        };
        let mount_point = mount_point.root;
        let mount_point = mount_point
            .downcast_arc::<RootShimDentry>()
            .map_err(|_| AlienError::EINVAL)
            .expect("mount point is not a shim dentry");
        let inode_id = mount_point.inode_id();
        let fs_domain_ident = mount_point.fs_domain_ident();
        let min_len = core::cmp::min(fs_domain_ident.len(), domain_ident.len());
        domain_ident.as_mut_slice()[..min_len]
            .copy_from_slice(&fs_domain_ident.as_slice()[..min_len]);
        Ok(Some((domain_ident, inode_id)))
    }

    fn dentry_clear_mount_point(&self, inode: InodeID) -> AlienResult<()> {
        let dentry = self.dentry_map.lock().index(&inode).clone();
        dentry.clear_mount_point();
        Ok(())
    }

    fn dentry_find(&self, inode: InodeID, name: &DVec<u8>) -> AlienResult<Option<InodeID>> {
        let dentry = self.dentry_map.lock().index(&inode).clone();
        let name = core::str::from_utf8(name.as_slice()).unwrap();
        let sub_dentry = dentry.find(name);
        let sub_dentry = match sub_dentry {
            Some(sub_dentry) => sub_dentry,
            None => return Ok(None),
        };
        let inode_id = self
            .inode_index
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        self.dentry_map.lock().insert(inode_id, sub_dentry);
        Ok(Some(inode_id))
    }

    fn dentry_remove(&self, inode: InodeID, name: &DVec<u8>) -> AlienResult<()> {
        let dentry = self.dentry_map.lock().index(&inode).clone();
        let name = core::str::from_utf8(name.as_slice()).unwrap();
        let _sub_dentry = dentry.remove(name);
        // println!("<dentry_remove> remove {} from {}", name, inode);
        Ok(())
    }

    fn read_at(
        &self,
        inode: InodeID,
        offset: u64,
        buf: DVec<u8>,
    ) -> AlienResult<(DVec<u8>, usize)> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        let (buf, r) = inode.read_at(offset, buf)?;
        Ok((buf, r))
    }

    fn write_at(&self, inode: InodeID, offset: u64, buf: &DVec<u8>) -> AlienResult<usize> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        let w = inode.write_at(offset, buf)?;
        Ok(w)
    }

    fn readdir(
        &self,
        inode: InodeID,
        start_index: usize,
        mut entry: DBox<DirEntryWrapper>,
    ) -> AlienResult<DBox<DirEntryWrapper>> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        let vfs_entry = inode.readdir(start_index)?;
        match vfs_entry {
            None => {
                entry.name_len = 0;
            }
            Some(vfs_entry) => {
                entry.name_len = vfs_entry.name.len();
                entry.ty = vfs_entry.ty;
                entry.ino = vfs_entry.ino;
                let copy_len = core::cmp::min(entry.name_len, entry.name.len());
                entry.name.as_mut_slice()[..copy_len]
                    .copy_from_slice(&vfs_entry.name.as_bytes()[..copy_len]);
            }
        }
        Ok(entry)
    }

    fn poll(&self, inode: InodeID, mask: VfsPollEvents) -> AlienResult<VfsPollEvents> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        let res = inode.poll(mask)?;
        Ok(res)
    }
    fn ioctl(&self, inode: InodeID, cmd: u32, arg: usize) -> AlienResult<usize> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        let res = inode.ioctl(cmd, arg)?;
        Ok(res)
    }

    fn flush(&self, inode: InodeID) -> AlienResult<()> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        inode.flush()?;
        Ok(())
    }

    fn fsync(&self, inode: InodeID) -> AlienResult<()> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        inode.fsync()?;
        Ok(())
    }

    fn rmdir(&self, parent: InodeID, name: &DVec<u8>) -> AlienResult<()> {
        let parent_dentry = self.dentry_map.lock().index(&parent).clone();
        let name = core::str::from_utf8(name.as_slice()).unwrap();
        let parent = parent_dentry.inode()?;
        parent.rmdir(name)?;
        parent_dentry.remove(name);
        Ok(())
    }

    fn node_permission(&self, inode: InodeID) -> AlienResult<VfsNodePerm> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        let perm = inode.node_perm();
        Ok(perm)
    }

    fn create(
        &self,
        parent: InodeID,
        name: &DVec<u8>,
        ty: VfsNodeType,
        perm: VfsNodePerm,
        rdev: Option<u64>,
    ) -> AlienResult<InodeID> {
        let parent = self.dentry_map.lock().index(&parent).clone();
        let parent_inode = parent.inode()?;
        let name = core::str::from_utf8(name.as_slice()).unwrap();
        let inode = parent_inode.create(name, ty, perm, rdev)?;
        let inode_id = self
            .inode_index
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let dentry = parent.insert(name, inode)?;
        self.dentry_map.lock().insert(inode_id, dentry);
        Ok(inode_id)
    }

    fn link(&self, parent: InodeID, name: &DVec<u8>, src: InodeID) -> AlienResult<InodeID> {
        let parent_dentry = self.dentry_map.lock().index(&parent).clone();
        let name = core::str::from_utf8(name.as_slice()).unwrap();
        let src_dentry = self.dentry_map.lock().index(&src).clone();
        let src = src_dentry.inode()?;
        let parent = parent_dentry.inode()?;
        let inode = parent.link(name, src)?;
        let inode_id = self
            .inode_index
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let dentry = parent_dentry.insert(name, inode)?;
        self.dentry_map.lock().insert(inode_id, dentry);
        Ok(inode_id)
    }

    fn unlink(&self, parent: InodeID, name: &DVec<u8>) -> AlienResult<()> {
        let parent_dentry = self.dentry_map.lock().index(&parent).clone();
        let name = core::str::from_utf8(name.as_slice()).unwrap();
        let parent = parent_dentry.inode()?;
        parent.unlink(name)?;
        parent_dentry.remove(name);
        // println!("<generic> The unlink implementation is not correct");
        Ok(())
    }

    fn symlink(
        &self,
        parent: InodeID,
        name: &DVec<u8>,
        link: &DVec<u8>,
    ) -> AlienResult<InodeID> {
        let parent_dentry = self.dentry_map.lock().index(&parent).clone();
        let name = core::str::from_utf8(name.as_slice()).unwrap();
        let link = core::str::from_utf8(link.as_slice()).unwrap();
        let parent = parent_dentry.inode()?;
        let inode = parent.symlink(name, link)?;
        let inode_id = self
            .inode_index
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let dentry = parent_dentry.insert(name, inode)?;
        self.dentry_map.lock().insert(inode_id, dentry);
        Ok(inode_id)
    }

    fn lookup(&self, parent: InodeID, name: &DVec<u8>) -> AlienResult<InodeID> {
        let parent_dentry = self.dentry_map.lock().index(&parent).clone();
        let name = core::str::from_utf8(name.as_slice()).unwrap();
        // println_color!(31, "<generic> lookup {:?} in dir {}", name, parent_dentry.name());
        let dentry = if let Some(dt) = parent_dentry.find(name) {
            dt
        } else {
            let parent = parent_dentry.inode()?;
            let inode = parent.lookup(name)?;

            parent_dentry.insert(name, inode)?
        };
        // println_color!(31, "<generic> lookup {:?} in dir success", name);
        let inode_id = self
            .inode_index
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        self.dentry_map.lock().insert(inode_id, dentry);
        Ok(inode_id)
    }

    fn readlink(&self, inode: InodeID, buf: DVec<u8>) -> AlienResult<(DVec<u8>, usize)> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        let (buf, l) = inode.readlink(buf)?;
        Ok((buf, l))
    }

    fn set_attr(&self, inode: InodeID, attr: InodeAttr) -> AlienResult<()> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        inode.set_attr(attr)?;
        Ok(())
    }

    fn get_attr(&self, inode: InodeID) -> AlienResult<VfsFileStat> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        let stat = inode.get_attr()?;
        Ok(stat)
    }

    fn inode_type(&self, inode: InodeID) -> AlienResult<VfsNodeType> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        let ty = inode.inode_type();
        Ok(ty)
    }

    fn truncate(&self, inode: InodeID, len: u64) -> AlienResult<()> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        inode.truncate(len)?;
        Ok(())
    }

    fn rename(
        &self,
        old_parent: InodeID,
        old_name: &DVec<u8>,
        new_parent: InodeID,
        new_name: &DVec<u8>,
        flags: VfsRenameFlag,
    ) -> AlienResult<()> {
        let old_parent_dt = self.dentry_map.lock().index(&old_parent).clone();
        let old_name = core::str::from_utf8(old_name.as_slice()).unwrap();
        let new_parent_dt = self.dentry_map.lock().index(&new_parent).clone();
        let new_name = core::str::from_utf8(new_name.as_slice()).unwrap();
        let old_parent = old_parent_dt.inode()?;
        let new_parent = new_parent_dt.inode()?;
        old_parent.rename_to(old_name, new_parent, new_name, flags)?;
        Ok(())
    }

    fn update_time(&self, inode: InodeID, time: VfsTime, now: VfsTimeSpec) -> AlienResult<()> {
        let inode = self.dentry_map.lock().index(&inode).inode()?;
        inode.update_time(time, now)?;
        Ok(())
    }

    fn sync_fs(&self, wait: bool) -> AlienResult<()> {
        let inode = self.dentry_map.lock().index(&0).inode()?;
        inode.get_super_block()?.sync_fs(wait)?;
        Ok(())
    }

    fn stat_fs(&self, mut fs_stat: DBox<VfsFsStat>) -> AlienResult<DBox<VfsFsStat>> {
        let inode = self.dentry_map.lock().index(&0).inode()?;
        let stat = inode.get_super_block()?.stat_fs()?;
        *fs_stat = stat;
        Ok(fs_stat)
    }

    fn super_type(&self) -> AlienResult<SuperType> {
        let inode = self.dentry_map.lock().index(&0).inode()?;
        let ty = inode.get_super_block()?.super_type();
        Ok(ty)
    }

    fn kill_sb(&self) -> AlienResult<()> {
        let inode = self.dentry_map.lock().index(&0).inode()?;
        let sb = inode.get_super_block()?;
        self.fs.kill_sb(sb)?;
        Ok(())
    }

    fn fs_flag(&self) -> AlienResult<FileSystemFlags> {
        Ok(self.fs.fs_flag())
    }

    fn fs_name(&self, mut name: DVec<u8>) -> AlienResult<(DVec<u8>, usize)> {
        let fs_name = self.fs.fs_name();
        let copy_len = core::cmp::min(name.len(), fs_name.len());
        name.as_mut_slice()[..copy_len].copy_from_slice(&fs_name.as_bytes()[..copy_len]);
        Ok((name, copy_len))
    }
    fn fs_magic(&self) -> AlienResult<u128> {
        let v = self.magic.get().unwrap();
        Ok(*v)
    }
}
define_unwind_for_FsDomain!(GenericFsDomain);
