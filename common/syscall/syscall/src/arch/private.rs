use basic::{AlienResult, constants::*};

use super::{
    super::{domain::*, gui::*},
    SysCallDomainImpl,
};

pub(super) fn dispatch(
    domain: &SysCallDomainImpl,
    syscall_id: usize,
    args: [usize; 6],
) -> Option<AlienResult<isize>> {
    let task = &domain.task_domain;
    let vfs = &domain.vfs_domain;
    let gpu = domain.gpu_domain.as_ref();
    let input = domain.input_domain.as_slice();

    let result = match syscall_id {
        // AsyncAlien 私有 syscall，不属于 Linux ABI。
        SYSCALL_LOAD_DOMAIN => sys_load_domain(task, vfs, args[0], args[1] as u8, args[2], args[3]),
        SYSCALL_REPLACE_DOMAIN => sys_replace_domain(task, args[0], args[1], args[2], args[3], args[4] as u8),
        SYSCALL_FRAMEBUFFER => sys_framebuffer(task, gpu),
        SYSCALL_FRAMEBUFFER_FLUSH => sys_framebuffer_flush(gpu),
        SYSCALL_EVENT_GET => sys_event_get(task, input, args[0], args[1]),
        _ => return None,
    };
    Some(result)
}
