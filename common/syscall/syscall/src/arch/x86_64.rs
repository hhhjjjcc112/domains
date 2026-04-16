use basic::{AlienResult, constants::*};

#[allow(unused_imports)]
use super::super::{
    domain::*, fs::*, gui::*, mm::*, signal::*, socket::*, system::*, task::*, time::*,
};
use super::{SysCallDomainImpl, dispatch_rest};

pub(super) fn dispatch(
    domain: &SysCallDomainImpl,
    syscall_id: usize,
    args: [usize; 6],
) -> AlienResult<isize> {
    let task = &domain.task_domain;
    let vfs = &domain.vfs_domain;

    match syscall_id {
        // x86_64 旧 ABI 的 open 等价于 openat(AT_FDCWD, ... )。
        SYSCALL_OPEN => sys_openat(vfs, task, AT_FDCWD as usize, args[0] as *const u8, args[1], args[2]),
        // x86_64 旧 ABI 的 access 等价于 faccessat(AT_FDCWD, ..., 0)。
        SYSCALL_ACCESS => sys_faccessat(vfs, task, AT_FDCWD as usize, args[0], args[1], 0),
        // stat/lstat 都通过 newfstatat 兼容，lstat 额外加 NOFOLLOW。
        SYSCALL_STAT => sys_fstatat(vfs, task, AT_FDCWD as usize, args[0] as *const u8, args[1], 0),
        SYSCALL_LSTAT => sys_fstatat(vfs, task, AT_FDCWD as usize, args[0] as *const u8, args[1], pconst::io::StatFlags::AT_SYMLINK_NOFOLLOW.bits() as usize),
        SYSCALL_MKDIR => sys_mkdirat(vfs, task, AT_FDCWD as usize, args[0], args[1]),
        // x86_64 的 renameat 无 flags，统一落到 renameat2(..., 0)。
        SYSCALL_RENAMEAT => sys_renameat2(vfs, task, args[0], args[1], args[2], args[3], 0),
        SYSCALL_PIPE => sys_pipe2(task, vfs, args[0], 0),
        // x86_64 旧 poll 入口复用 ppoll 实现，并带上兼容占位 sigmask。
        SYSCALL_POLL => sys_ppoll(vfs, task, args[0], args[1], args[2], PPOLL_FROM_POLL_SIGMASK),
        SYSCALL_SELECT => sys_select(vfs, task, args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_ARCH_PRCTL => sys_arch_prctl(task, args[0], args[1]),
        // x86_64 clone 的 tls/child_tid 顺序与 riscv64 不同，这里做 ABI 重排。
        SYSCALL_CLONE => sys_clone(task, args[0], args[1], args[2], args[4], args[3]),
        // fork/vfork 都只是 clone 的 x86_64 兼容入口。
        SYSCALL_FORK => sys_clone(task, signal::SignalNumber::SIGCHLD as usize, 0, 0, 0, 0),
        SYSCALL_VFORK => sys_clone(task, (task::CloneFlags::CLONE_VFORK | task::CloneFlags::CLONE_VM).bits() as usize | signal::SignalNumber::SIGCHLD as usize, 0, 0, 0, 0),
        SYSCALL_GETPGRP => sys_get_pgrp(task),
        _ => dispatch_rest(domain, syscall_id, args),
    }
}
