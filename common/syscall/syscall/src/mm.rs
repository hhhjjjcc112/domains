use alloc::sync::Arc;

use basic::{
    config::FRAME_SIZE,
    constants::io::{MMapFlags, MMapType, ProtFlags, MMAP_TYPE_MASK},
    AlienError, AlienResult,
};
use interface::{TaskDomain, TmpHeapInfo, VfsDomain};
use log::info;
use shared_heap::DBox;

/// brk：`addr` 为目标堆顶；传 0 时返回当前堆顶。
pub fn sys_brk(
    _vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    addr: usize,
) -> AlienResult<isize> {
    let heap_info = DBox::new(TmpHeapInfo::default());
    let heap_info = task_domain.heap_info(heap_info)?;
    if addr == 0 {
        return Ok(heap_info.current as isize);
    }
    if addr < heap_info.start || addr < heap_info.current {
        // panic!("heap can't be shrinked");
        return Err(AlienError::EINVAL);
    }
    task_domain.do_brk(addr)
}

/// mmap：`addr/len/prot/flags/fd/offset` 对应 Linux mmap 参数。
pub fn sys_mmap(
    task_domain: &Arc<dyn TaskDomain>,
    addr: usize,
    len: usize,
    prot: usize,
    flags: usize,
    fd: usize,
    offset: usize,
) -> AlienResult<isize> {
    if offset % FRAME_SIZE != 0 {
        return Err(AlienError::EINVAL);
    }
    let prot = ProtFlags::from_bits_truncate(prot as _);
    let _ty = MMapType::try_from((flags as u32 & MMAP_TYPE_MASK) as u8)
        .map_err(|_| AlienError::EINVAL)?;
    let flags = MMapFlags::from_bits_truncate(flags as u32);

    if flags.contains(MMapFlags::MAP_ANONYMOUS) && offset != 0 {
        return Err(AlienError::EINVAL);
    }
    info!(
        "mmap: start: {:#x}, len: {:#x}, prot: {:?}, flags: {:?}, fd: {}, offset: {:#x}",
        addr, len, prot, flags, fd, offset
    );
    let res = task_domain.do_mmap(addr, len, prot.bits(), flags.bits(), fd, offset);
    info!("mmap: res: {:#x?}", res);
    res
}

/// munmap：`addr` 是映射起始地址，`len` 是长度。
pub fn sys_unmap(task_domain: &Arc<dyn TaskDomain>, addr: usize, len: usize) -> AlienResult<isize> {
    task_domain.do_munmap(addr, len)
}

/// mprotect：`addr/len` 指定区间，`prot` 是新的保护位。
pub fn sys_mprotect(
    task_domain: &Arc<dyn TaskDomain>,
    addr: usize,
    len: usize,
    prot: usize,
) -> AlienResult<isize> {
    let prot = ProtFlags::from_bits_truncate(prot as _);
    info!(
        "mprotect: addr: {:#x}, len: {:#x}, prot: {:?}",
        addr, len, prot
    );
    task_domain.do_mprotect(addr, len, prot.bits())
}
