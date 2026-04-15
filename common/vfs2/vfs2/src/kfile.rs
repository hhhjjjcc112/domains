use alloc::{format, sync::Arc, vec::Vec};
use core::fmt::{Debug, Formatter};

use basic::{
    constants::{
        io::{Dirent64, DirentType, OpenFlags, PollEvents, SeekFrom},
        LinuxErrno,
    },
    sync::Mutex,
    AlienResult,
};
use downcast_rs::{impl_downcast, DowncastSync};
use interface::InodeID;
use shared_heap::DVec;
use storage::CustomStorge;
use vfs_common::meta::KernelFileMeta;
use vfscore::{
    dentry::VfsDentry,
    error::VfsError,
    inode::VfsInode,
    path::VfsPath,
    utils::{VfsFileStat, VfsNodeType, VfsPollEvents},
};

use crate::{shim::FsShimInode, system_root_fs};

pub struct KernelFile {
    inode_id: u64,
    meta: KMeta,
    dentry: Arc<dyn VfsDentry>,
}

pub type KMeta = Arc<Mutex<KernelFileMeta>, CustomStorge>;

impl Debug for KernelFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KernelFile")
            .field("meta", &self.meta)
            .field("name", &self.dentry.name())
            .finish()
    }
}
impl KernelFile {
    pub fn new(dentry: Arc<dyn VfsDentry>, open_flag: OpenFlags, inode_id: InodeID) -> Self {
        let key = format!("kfile_{}", inode_id);
        let meta = storage::get_or_insert(&key, || {
            let pos = if open_flag.contains(OpenFlags::O_APPEND) {
                dentry.inode().unwrap().get_attr().unwrap().st_size
            } else {
                0
            };
            let inode = dentry
                .inode()
                .unwrap()
                .downcast_arc::<FsShimInode>()
                .map_err(|_| LinuxErrno::EINVAL)
                .unwrap();
            let real_inode_id = inode.inode_id();
            let fs_domain = inode.fs_domain();
            let ident = inode.fs_domain_ident();
            let mut fs_domain_ident = Vec::new_in(CustomStorge);
            fs_domain_ident.extend_from_slice(&ident);
            Mutex::new(KernelFileMeta::new(
                pos,
                open_flag,
                real_inode_id,
                fs_domain,
                fs_domain_ident,
            ))
        });
        Self {
            meta,
            dentry,
            inode_id,
        }
    }

    pub fn from_meta(dentry: Arc<dyn VfsDentry>, meta: KMeta, inode_id: InodeID) -> Self {
        Self {
            meta,
            dentry,
            inode_id,
        }
    }
}

#[allow(dead_code)]
pub trait File: DowncastSync + Debug {
    fn read(&self, buf: DVec<u8>) -> AlienResult<(DVec<u8>, usize)>;
    fn write(&self, buf: &DVec<u8>) -> AlienResult<usize>;
    fn read_at(&self, _offset: u64, _buf: DVec<u8>) -> AlienResult<(DVec<u8>, usize)> {
        Err(LinuxErrno::ENOSYS)
    }
    fn write_at(&self, _offset: u64, _buf: &DVec<u8>) -> AlienResult<usize> {
        Err(LinuxErrno::ENOSYS)
    }
    fn flush(&self) -> AlienResult<()> {
        Ok(())
    }
    fn fsync(&self) -> AlienResult<()> {
        Ok(())
    }
    fn seek(&self, pos: SeekFrom) -> AlienResult<u64>;
    /// Gets the file attributes.
    fn get_attr(&self) -> AlienResult<VfsFileStat>;
    fn ioctl(&self, _cmd: u32, _arg: usize) -> AlienResult<usize> {
        Err(LinuxErrno::ENOSYS)
    }
    fn set_open_flag(&self, _flag: OpenFlags) {}
    fn get_open_flag(&self) -> OpenFlags {
        OpenFlags::O_RDONLY
    }
    fn dentry(&self) -> Arc<dyn VfsDentry>;
    fn inode(&self) -> Arc<dyn VfsInode>;
    fn readdir(&self, _buf: &mut [u8]) -> AlienResult<usize> {
        Err(LinuxErrno::ENOSYS)
    }
    fn truncate(&self, _len: u64) -> AlienResult<()> {
        Err(LinuxErrno::ENOSYS)
    }
    fn is_readable(&self) -> bool;
    fn is_writable(&self) -> bool;
    fn is_append(&self) -> bool;
    fn poll(&self, _event: PollEvents) -> AlienResult<PollEvents> {
        Err(LinuxErrno::ENOSYS)
    }
}

impl_downcast!(sync  File);

impl File for KernelFile {
    fn read(&self, buf: DVec<u8>) -> AlienResult<(DVec<u8>, usize)> {
        let pos = self.meta.lock().pos;
        let (buf, read) = self.read_at(pos, buf)?;
        self.meta.lock().pos += read as u64;
        Ok((buf, read))
    }
    fn write(&self, buf: &DVec<u8>) -> AlienResult<usize> {
        let pos = self.meta.lock().pos;
        let write = self.write_at(pos, buf)?;
        self.meta.lock().pos += write as u64;
        Ok(write)
    }
    fn read_at(&self, offset: u64, buf: DVec<u8>) -> AlienResult<(DVec<u8>, usize)> {
        if buf.is_empty() {
            return Ok((buf, 0));
        }
        let open_flag = self.meta.lock().open_flag;
        if !open_flag.contains(OpenFlags::O_RDONLY) && !open_flag.contains(OpenFlags::O_RDWR) {
            return Err(LinuxErrno::EPERM);
        }
        let inode = self.dentry.inode()?;
        let res = inode.read_at(offset, buf)?;
        Ok(res)
    }

    fn write_at(&self, offset: u64, buf: &DVec<u8>) -> AlienResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let open_flag = self.meta.lock().open_flag;
        if !open_flag.contains(OpenFlags::O_WRONLY) && !open_flag.contains(OpenFlags::O_RDWR) {
            return Err(LinuxErrno::EPERM);
        }
        let inode = self.dentry.inode()?;
        let write = inode.write_at(offset, buf)?;
        Ok(write)
    }

    fn flush(&self) -> AlienResult<()> {
        let open_flag = self.meta.lock().open_flag;
        if !open_flag.contains(OpenFlags::O_WRONLY) & !open_flag.contains(OpenFlags::O_RDWR) {
            return Err(LinuxErrno::EPERM);
        }
        let inode = self.dentry.inode()?;
        inode.flush()?;
        Ok(())
    }

    fn fsync(&self) -> AlienResult<()> {
        let open_flag = self.meta.lock().open_flag;
        if !open_flag.contains(OpenFlags::O_WRONLY) && !open_flag.contains(OpenFlags::O_RDWR) {
            return Err(LinuxErrno::EPERM);
        }
        let inode = self.dentry.inode()?;
        inode.fsync()?;
        Ok(())
    }

    /// check for special file
    fn seek(&self, pos: SeekFrom) -> AlienResult<u64> {
        let spos = &mut self.meta.lock().pos;
        let size = self.get_attr()?.st_size;
        let new_offset = match pos {
            SeekFrom::Start(pos) => Some(pos),
            SeekFrom::Current(off) => spos.checked_add_signed(off),
            SeekFrom::End(off) => size.checked_add_signed(off),
        }
        .ok_or(VfsError::Invalid)?;
        *spos = new_offset;
        Ok(new_offset)
    }

    /// Gets the file attributes.
    fn get_attr(&self) -> AlienResult<VfsFileStat> {
        self.dentry.inode()?.get_attr().map_err(Into::into)
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> AlienResult<usize> {
        let inode = self.dentry.inode().unwrap();
        inode.ioctl(_cmd, _arg).map_err(Into::into)
    }

    fn set_open_flag(&self, flag: OpenFlags) {
        self.meta.lock().open_flag = flag;
    }

    fn get_open_flag(&self) -> OpenFlags {
        self.meta.lock().open_flag
    }
    fn dentry(&self) -> Arc<dyn VfsDentry> {
        self.dentry.clone()
    }
    fn inode(&self) -> Arc<dyn VfsInode> {
        self.dentry.inode().unwrap()
    }
    fn readdir(&self, buf: &mut [u8]) -> AlienResult<usize> {
        let inode = self.inode();
        let pos = &mut self.meta.lock().pos;
        let mut count = 0;
        loop {
            let dirent = inode.readdir(*pos as usize).map_err(|e| {
                *pos = 0;
                e
            })?;
            match dirent {
                Some(d) => {
                    let dirent64 =
                        Dirent64::new(&d.name, d.ino, *pos as i64, vfsnodetype2dirent64(d.ty));
                    if count + dirent64.len() <= buf.len() {
                        let slice = dirent64.as_slice();
                        buf[count..count + slice.len()].copy_from_slice(slice);
                        let mut name = d.name.clone();
                        name.push('\0');
                        let len = name.as_bytes().len();
                        buf[count + dirent64.name_offset()..count + dirent64.name_offset() + len]
                            .copy_from_slice(name.as_bytes());
                        count += dirent64.len();
                    } else {
                        break;
                    } // Buf is small
                }
                None => {
                    break;
                } // EOF
            }
            *pos += 1;
        }
        Ok(count)
    }
    fn truncate(&self, len: u64) -> AlienResult<()> {
        let open_flag = self.meta.lock().open_flag;
        if !open_flag.contains(OpenFlags::O_WRONLY) & !open_flag.contains(OpenFlags::O_RDWR) {
            return Err(LinuxErrno::EINVAL);
        }
        let dt = self.dentry();
        VfsPath::new(system_root_fs(), dt)
            .truncate(len)
            .map_err(Into::into)
    }
    fn is_readable(&self) -> bool {
        let open_flag = self.meta.lock().open_flag;
        open_flag.contains(OpenFlags::O_RDONLY) | open_flag.contains(OpenFlags::O_RDWR)
    }
    fn is_writable(&self) -> bool {
        let open_flag = self.meta.lock().open_flag;
        open_flag.contains(OpenFlags::O_WRONLY) | open_flag.contains(OpenFlags::O_RDWR)
    }

    fn is_append(&self) -> bool {
        let open_flag = self.meta.lock().open_flag;
        open_flag.contains(OpenFlags::O_APPEND)
    }

    fn poll(&self, event: PollEvents) -> AlienResult<PollEvents> {
        let inode = self.dentry.inode()?;
        let res = inode
            .poll(VfsPollEvents::from_bits_truncate(event.bits() as u16))
            .map(|e| PollEvents::from_bits_truncate(e.bits() as u32));
        res.map_err(Into::into)
    }
}

fn vfsnodetype2dirent64(ty: VfsNodeType) -> DirentType {
    DirentType::from_u8(ty as u8)
}

impl Drop for KernelFile {
    fn drop(&mut self) {
        // let _ = self.flush();
        // let _ = self.fsync();
        let inode_id = self.inode_id;
        let key = format!("kfile_{}", inode_id);
        // basic::println!("drop KernelFile: {}", key);
        let meta = storage::remove::<Mutex<KernelFileMeta>>(&key);
        assert!(meta.is_some());
    }
}
