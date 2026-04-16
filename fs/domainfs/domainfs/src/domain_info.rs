use alloc::{
    format,
    string::{String, ToString},
    sync::Arc,
};
use core::cmp::min;

use interface::DomainTypeRaw;
use shared_heap::DVec;
use vfscore::{
    VfsResult,
    error::VfsError,
    file::VfsFile,
    inode::VfsInode,
    utils::{VfsDirEntry, VfsFileStat, VfsInodeMode, VfsNodePerm, VfsNodeType, VfsTimeSpec},
};

use crate::{DOMAIN_INFO, custom_inode::CustomRootInode};

pub fn domain_fs_root() -> Arc<CustomRootInode> {
    let root = CustomRootInode::new();
    root.insert_inode("domain-type".to_string(), Arc::new(DomainTyInfoDir));
    root.insert_inode("domains".to_string(), Arc::new(DomainFileInfoDir));
    Arc::new(root)
}

pub struct DomainTyInfoDir;

fn __readdir_from_ty(start_index: usize) -> VfsResult<Option<VfsDirEntry>> {
    let domain_info = DOMAIN_INFO.get();
    if let Some(domain_info) = domain_info {
        let guard = domain_info.lock();
        let tys = &guard.ty_list;
        let entry = tys.iter().nth(start_index).map(|(ty, _)| {
            let name = format!("{}", ty);
            VfsDirEntry {
                ino: 0,
                ty: VfsNodeType::File,
                name,
            }
        });
        Ok(entry)
    } else {
        Ok(None)
    }
}
fn __readdir_from_domain_list(start_index: usize) -> VfsResult<Option<VfsDirEntry>> {
    let domain_info = DOMAIN_INFO.get();
    if let Some(domain_info) = domain_info {
        let guard = domain_info.lock();
        let domains = &guard.domain_list;
        let entry = domains
            .iter()
            .nth(start_index)
            .map(|(_id, info)| VfsDirEntry {
                ino: 0,
                ty: VfsNodeType::File,
                name: info.name.to_string(),
            });
        Ok(entry)
    } else {
        Ok(None)
    }
}

impl VfsFile for DomainTyInfoDir {
    fn readdir(&self, start_index: usize) -> VfsResult<Option<VfsDirEntry>> {
        __readdir_from_ty(start_index)
    }
}

impl VfsInode for DomainTyInfoDir {
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VfsInode>> {
        let domain_info = DOMAIN_INFO.get();
        if let Some(domain_info) = domain_info {
            let guard = domain_info.lock();
            let tys = &guard.ty_list;
            let ty = domain_type_from_str(name).ok_or(VfsError::NoEntry)?;
            let _ = tys.get(&ty).ok_or(VfsError::NoEntry)?;
            return Ok(Arc::new(DomainInfoFile::new(
                DomainInfoFileType::DomainType(ty),
            )));
        }
        Err(VfsError::NoEntry)
    }
    fn get_attr(&self) -> VfsResult<VfsFileStat> {
        let mode = VfsInodeMode::from(VfsNodePerm::from_bits_truncate(0o644), VfsNodeType::Dir);
        Ok(VfsFileStat {
            st_dev: 0,
            st_ino: 0,
            st_mode: mode.bits(),
            st_nlink: 1,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            __pad: 0,
            st_size: 4096,
            st_blksize: 512,
            __pad2: 0,
            st_blocks: 0,
            st_atime: VfsTimeSpec::default(),
            st_mtime: VfsTimeSpec::default(),
            st_ctime: VfsTimeSpec::default(),
            unused: 0,
        })
    }
    fn inode_type(&self) -> VfsNodeType {
        VfsNodeType::Dir
    }
}

pub struct DomainFileInfoDir;

impl VfsFile for DomainFileInfoDir {
    fn readdir(&self, start_index: usize) -> VfsResult<Option<VfsDirEntry>> {
        __readdir_from_domain_list(start_index)
    }
}

impl VfsInode for DomainFileInfoDir {
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VfsInode>> {
        let domain_info = DOMAIN_INFO.get();
        if let Some(domain_info) = domain_info {
            let guard = domain_info.lock();
            let domain_list = &guard.domain_list;
            let _ = domain_list
                .iter()
                .find(|(_id, info)| info.name == name)
                .ok_or(VfsError::NoEntry)?;
            return Ok(Arc::new(DomainInfoFile::new(DomainInfoFileType::Domain(
                name.to_string(),
            ))));
        }
        Err(VfsError::NoEntry)
    }
    fn get_attr(&self) -> VfsResult<VfsFileStat> {
        let mode = VfsInodeMode::from(VfsNodePerm::from_bits_truncate(0o644), VfsNodeType::Dir);
        Ok(VfsFileStat {
            st_dev: 0,
            st_ino: 0,
            st_mode: mode.bits(),
            st_nlink: 1,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            __pad: 0,
            st_size: 4096,
            st_blksize: 512,
            __pad2: 0,
            st_blocks: 0,
            st_atime: VfsTimeSpec::default(),
            st_mtime: VfsTimeSpec::default(),
            st_ctime: VfsTimeSpec::default(),
            unused: 0,
        })
    }
    fn inode_type(&self) -> VfsNodeType {
        VfsNodeType::Dir
    }
}

fn domain_type_from_str(name: &str) -> Option<DomainTypeRaw> {
    match name {
        "FsDomain" => Some(DomainTypeRaw::FsDomain),
        "BlkDeviceDomain" => Some(DomainTypeRaw::BlkDeviceDomain),
        "CacheBlkDeviceDomain" => Some(DomainTypeRaw::CacheBlkDeviceDomain),
        "RtcDomain" => Some(DomainTypeRaw::RtcDomain),
        "GpuDomain" => Some(DomainTypeRaw::GpuDomain),
        "InputDomain" => Some(DomainTypeRaw::InputDomain),
        "VfsDomain" => Some(DomainTypeRaw::VfsDomain),
        "UartDomain" => Some(DomainTypeRaw::UartDomain),
        #[cfg(target_arch = "riscv64")]
        "PLICDomain" => Some(DomainTypeRaw::PLICDomain),
        "TaskDomain" => Some(DomainTypeRaw::TaskDomain),
        "SysCallDomain" => Some(DomainTypeRaw::SysCallDomain),
        "ShadowBlockDomain" => Some(DomainTypeRaw::ShadowBlockDomain),
        "BufUartDomain" => Some(DomainTypeRaw::BufUartDomain),
        "NetDeviceDomain" => Some(DomainTypeRaw::NetDeviceDomain),
        "BufInputDomain" => Some(DomainTypeRaw::BufInputDomain),
        "EmptyDeviceDomain" => Some(DomainTypeRaw::EmptyDeviceDomain),
        #[cfg(target_arch = "x86_64")]
        "APICDomain" => Some(DomainTypeRaw::APICDomain),
        "DevFsDomain" => Some(DomainTypeRaw::DevFsDomain),
        "SchedulerDomain" => Some(DomainTypeRaw::SchedulerDomain),
        "LogDomain" => Some(DomainTypeRaw::LogDomain),
        "NetDomain" => Some(DomainTypeRaw::NetDomain),
        _ => None,
    }
}

pub enum DomainInfoFileType {
    DomainType(DomainTypeRaw),
    Domain(String),
}
pub struct DomainInfoFile {
    ty: DomainInfoFileType,
}

fn domain_ty_data(ty: DomainTypeRaw) -> String {
    let domain_info = DOMAIN_INFO.get().unwrap();
    let guard = domain_info.lock();
    let tys = &guard.ty_list;
    let infos = tys.get(&ty).unwrap();
    let data = format!("type: {}\nfile: {:#?}", ty, infos);
    data
}

fn domain_data(name: &str) -> String {
    let domain_info = DOMAIN_INFO.get().unwrap();
    let guard = domain_info.lock();
    let domain_list = &guard.domain_list;
    let info = domain_list
        .iter()
        .find(|(_id, info)| info.name == name)
        .unwrap()
        .1;
    let data = format!("DomainName: {}\nInformation: {:#?}", name, info);
    data
}

impl DomainInfoFile {
    pub fn new(ty: DomainInfoFileType) -> Self {
        Self { ty }
    }

    pub fn data(&self) -> String {
        match &self.ty {
            DomainInfoFileType::DomainType(ty) => domain_ty_data(*ty),
            DomainInfoFileType::Domain(name) => domain_data(name),
        }
    }
}

impl VfsFile for DomainInfoFile {
    fn read_at(&self, offset: u64, mut buf: DVec<u8>) -> VfsResult<(DVec<u8>, usize)> {
        let data = self.data();
        if offset as usize >= data.len() {
            return Ok((buf, 0));
        }
        let copy_start = min(offset as usize, data.len());
        let copy_end = min(copy_start + buf.len(), data.len());
        let copied = copy_end - copy_start;
        buf.as_mut_slice()[..copied].copy_from_slice(&data.as_bytes()[copy_start..copy_end]);
        Ok((buf, copied))
    }
}

impl VfsInode for DomainInfoFile {
    fn get_attr(&self) -> VfsResult<VfsFileStat> {
        let mode = VfsInodeMode::from(VfsNodePerm::from_bits_truncate(0o644), VfsNodeType::File);
        Ok(VfsFileStat {
            st_dev: 0,
            st_ino: 0,
            st_mode: mode.bits(),
            st_nlink: 1,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            __pad: 0,
            st_size: self.data().len() as u64,
            st_blksize: 512,
            __pad2: 0,
            st_blocks: 0,
            st_atime: VfsTimeSpec::default(),
            st_mtime: VfsTimeSpec::default(),
            st_ctime: VfsTimeSpec::default(),
            unused: 0,
        })
    }
    fn inode_type(&self) -> VfsNodeType {
        VfsNodeType::File
    }
}
