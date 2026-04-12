use alloc::sync::Arc;

use basic::{
    constants::{
        epoll::{EpollEvent, EpollEventType},
        io::OpenFlags,
    },
    AlienResult,
};
use interface::{TaskDomain, VfsDomain};
use pod::Pod;
use shared_heap::DBox;

/// epoll_create1：`flags` 是创建标志，返回新的 epoll fd。
pub fn sys_poll_createl(
    vfs_domain: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    flags: usize,
) -> AlienResult<isize> {
    let flags = OpenFlags::from_bits_truncate(flags);
    // println_color!(32, "poll_createl: flags: {:?}", flags);
    let epoll_file = vfs_domain.do_poll_create(flags.bits())?;
    let fd = task_domain.add_fd(epoll_file)?;
    Ok(fd as isize)
}

/// epoll_ctl 的用户态事件布局；`events` 是事件掩码，`data` 是用户数据。
#[derive(Pod, Copy, Clone)]
#[repr(C)]
pub struct EpollEventTmp {
    pub events: EpollEventType,
    pub data: u64,
}

/// epoll_ctl：`epfd` 是 epoll fd，`op` 是操作码，`fd` 是目标 fd，`event_ptr` 是用户态事件指针。
pub fn sys_poll_ctl(
    vfs_domain: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    epfd: usize,
    op: usize,
    fd: usize,
    event_ptr: usize,
) -> AlienResult<isize> {
    let event = task_domain.read_val_from_user::<EpollEventTmp>(event_ptr)?;
    let event = EpollEvent {
        events: event.events,
        data: event.data,
    };
    let inode = task_domain.get_fd(epfd)?;
    vfs_domain.do_poll_ctl(inode, op as u32, fd, DBox::new(event))?;
    Ok(0)
}

/// eventfd2：`init_val` 是初始计数，`flags` 是创建标志。
pub fn sys_eventfd2(
    vfs_domain: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    init_val: usize,
    flags: usize,
) -> AlienResult<isize> {
    let eventfd_file = vfs_domain.do_eventfd(init_val as u32, flags as u32)?;
    let fd = task_domain.add_fd(eventfd_file)?;
    Ok(fd as isize)
}
