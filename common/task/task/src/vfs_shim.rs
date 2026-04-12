use alloc::{sync::Arc, vec::Vec};

use basic::{
    constants::{io::OpenFlags, AT_FDCWD},
    println,
    AlienError, AlienResult,
};
use interface::{InodeID, VfsDomain, VFS_ROOT_ID, VFS_STDIN_ID, VFS_STDOUT_ID};
use shared_heap::{DBox, DVec};
use spin::{Lazy, Once};
use vfscore::utils::VfsFileStat;
use vfscore::utils::VfsInodeMode;

use crate::processor::current_task;

static VFS_DOMAIN: Once<Arc<dyn VfsDomain>> = Once::new();

pub fn init_vfs_domain(vfs_domain: Arc<dyn VfsDomain>) {
    VFS_DOMAIN.call_once(|| vfs_domain);
}

pub static STDIN: Lazy<Arc<ShimFile>> = Lazy::new(|| Arc::new(ShimFile::new(VFS_STDIN_ID)));

pub static STDOUT: Lazy<Arc<ShimFile>> = Lazy::new(|| Arc::new(ShimFile::new(VFS_STDOUT_ID)));

/// equal to Arc<dyn VfsDentry>
#[derive(Debug)]
pub struct ShimFile {
    id: InodeID,
}

impl ShimFile {
    pub const fn new(id: InodeID) -> Self {
        Self { id }
    }
    pub fn inode_id(&self) -> InodeID {
        self.id
    }

    fn get_attr(&self) -> AlienResult<DBox<VfsFileStat>> {
        let attr = DBox::<VfsFileStat>::new_uninit();
        let res = VFS_DOMAIN.get().unwrap().vfs_getattr(self.id, attr);
        res
    }

    fn read_at(&self, offset: u64, buf: DVec<u8>) -> AlienResult<(DVec<u8>, usize)> {
        let res = VFS_DOMAIN.get().unwrap().vfs_read_at(self.id, offset, buf);
        res
    }
}

impl Drop for ShimFile {
    fn drop(&mut self) {
        let _ = VFS_DOMAIN.get().unwrap().vfs_close(self.id);
    }
}

fn read_all_inner(file_name: &str, buf: &mut Vec<u8>, require_exec: bool) -> AlienResult<()> {
    let task = current_task();
    let path = if task.is_none() {
        (VFS_ROOT_ID, VFS_ROOT_ID)
    } else {
        user_path_at(AT_FDCWD, file_name)?
    };
    let name = DVec::from_slice(file_name.as_bytes());
    let file_id = VFS_DOMAIN.get().unwrap().vfs_open(
        path.1,
        &name,
        name.len(),
        0,
        OpenFlags::O_RDONLY.bits(),
    )?;
    let shim_file = ShimFile::new(file_id);
    let attr = shim_file.get_attr()?;
    if require_exec {
        let mode = VfsInodeMode::from_bits_truncate(attr.st_mode as u32);
        if !mode.intersects(
            VfsInodeMode::OWNER_EXEC | VfsInodeMode::GROUP_EXEC | VfsInodeMode::OTHER_EXEC,
        ) {
            warn!(
                "exec file {} denied: mode={:#o}",
                file_name,
                attr.st_mode & 0o777
            );
            return Err(AlienError::EACCES);
        }
    }
    let size = attr.st_size;
    let mut offset = 0;
    let mut tmp = DVec::new_uninit(1024);
    let mut res;
    while offset < size {
        (tmp, res) = shim_file.read_at(offset, tmp)?;
        if res == 0 {
            log::warn!(
                "read_all short read: file={}, offset={}, size={}",
                file_name,
                offset,
                size
            );
            return Err(AlienError::EIO);
        }
        offset += res as u64;
        buf.extend_from_slice(&tmp.as_slice()[..res]);
    }
    assert_eq!(offset, size);
    Ok(())
}

pub fn read_all(file_name: &str, buf: &mut Vec<u8>) -> bool {
    let res = read_all_inner(file_name, buf, false);
    if res.is_err() {
        println!("open/read file {} failed, err:{:?}", file_name, res.err());
        return false;
    }
    true
}

pub fn read_exec_all(file_name: &str, buf: &mut Vec<u8>) -> AlienResult<()> {
    read_all_inner(file_name, buf, true)
}

fn user_path_at(fd: isize, path: &str) -> AlienResult<(InodeID, InodeID)> {
    info!("user_path_at fd: {},path:{}", fd, path);
    let task = current_task().unwrap();
    let res = if !path.starts_with('/') {
        if fd == AT_FDCWD {
            let fs_context = &task.inner().fs_info;
            (VFS_ROOT_ID, fs_context.cwd.id)
        } else {
            let fd = fd as usize;
            let file = task.get_file(fd).ok_or(AlienError::EBADF)?;
            (VFS_ROOT_ID, file.inode_id())
        }
    } else {
        (VFS_ROOT_ID, VFS_ROOT_ID)
    };
    Ok(res)
}
