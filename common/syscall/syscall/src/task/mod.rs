mod ipc;
mod resource;

use alloc::sync::Arc;

use basic::{AlienError, AlienResult};
use basic::constants::task::{P_ALL, P_PID, P_PIDFD, P_PGID};
use interface::TaskDomain;
pub use ipc::*;
use log::info;
pub use resource::*;

#[cfg(target_arch = "x86_64")]
const ARCH_SET_GS: usize = 0x1001;
#[cfg(target_arch = "x86_64")]
const ARCH_SET_FS: usize = 0x1002;
#[cfg(target_arch = "x86_64")]
const ARCH_GET_FS: usize = 0x1003;
#[cfg(target_arch = "x86_64")]
const ARCH_GET_GS: usize = 0x1004;

/// clone：`flag` 是 clone 标志，`stack/ptid/tls/ctid` 分别是栈和线程相关用户态参数。
pub fn sys_clone(
    task_domain: &Arc<dyn TaskDomain>,
    flag: usize,
    stack: usize,
    ptid: usize,
    tls: usize,
    ctid: usize,
) -> AlienResult<isize> {
    task_domain.do_clone(flag, stack, ptid, tls, ctid)
}

/// wait4：`pid` 指定等待目标，`status` 和 `rusage` 是用户态输出，`options` 是等待选项。
pub fn sys_wait4(
    task_domain: &Arc<dyn TaskDomain>,
    pid: usize,
    status: usize,
    options: usize,
    rusage: usize,
) -> AlienResult<isize> {
    task_domain.do_wait4(pid as isize, status, options as u32, rusage)
}

/// waitid：`which/pid` 选择等待对象，`infop` 为 siginfo 输出，`options/rusage` 为等待参数。
pub fn sys_waitid(
    task_domain: &Arc<dyn TaskDomain>,
    which: usize,
    pid: usize,
    infop: usize,
    options: usize,
    rusage: usize,
) -> AlienResult<isize> {
    const CLD_EXITED: i32 = 1;

    if infop == 0 {
        return Err(AlienError::EFAULT);
    }

    let allowed = basic::constants::task::WaitOptions::WNOHANG
        | basic::constants::task::WaitOptions::WUNTRACED
        | basic::constants::task::WaitOptions::WEXITED
        | basic::constants::task::WaitOptions::WCONTINUED
        | basic::constants::task::WaitOptions::WNOWAIT;
    let raw_options = options as u32;
    if raw_options & !allowed.bits() != 0 {
        return Err(AlienError::EINVAL);
    }
    if raw_options & basic::constants::task::WaitOptions::WEXITED.bits() == 0 {
        return Err(AlienError::EINVAL);
    }

    // 目前只支持最常用的 P_ALL / P_PID，其他 Linux idtype 先保留为 ENOSYS。
    let wait_pid = match which {
        P_ALL => -1,
        P_PID => pid as isize,
        P_PGID | P_PIDFD => return Err(AlienError::ENOSYS),
        _ => return Err(AlienError::ENOSYS),
    };

    let waited_pid = task_domain.do_wait4(wait_pid, 0, raw_options, rusage)?;

    let mut siginfo = basic::constants::signal::SigInfo::default();
    if waited_pid == 0 {
        siginfo.si_signo = 0;
        siginfo.si_errno = 0;
        siginfo.si_code = 0;
    } else {
        siginfo.si_signo = basic::constants::signal::SignalNumber::SIGCHLD as i32;
        siginfo.si_errno = 0;
        siginfo.si_code = CLD_EXITED;
    }
    task_domain.write_val_to_user(infop, &siginfo)?;
    Ok(0)
}

/// execve：`filename_ptr/argv_ptr/envp_ptr` 分别是程序名、参数数组和环境数组指针。
pub fn sys_execve(
    task_domain: &Arc<dyn TaskDomain>,
    filename_ptr: usize,
    argv_ptr: usize,
    envp_ptr: usize,
) -> AlienResult<isize> {
    task_domain.do_execve(filename_ptr, argv_ptr, envp_ptr)
}

/// yield：主动让出当前 CPU。
pub fn sys_yield() -> AlienResult<isize> {
    basic::yield_now()?;
    Ok(0)
}

/// set_tid_address：`tidptr` 是线程退出时需要回写 TID 的用户地址。
pub fn sys_set_tid_address(task_domain: &Arc<dyn TaskDomain>, tidptr: usize) -> AlienResult<isize> {
    task_domain.do_set_tid_address(tidptr)
}

/// umask：读取当前掩码并设置新掩码，返回旧值。
pub fn sys_umask(task_domain: &Arc<dyn TaskDomain>, mask: usize) -> AlienResult<isize> {
    task_domain.do_umask(mask as u32).map(|old| old as isize)
}

#[cfg(target_arch = "x86_64")]
/// arch_prctl：x86_64 的 FS/GS 基址控制；`code` 决定操作，`addr` 是用户态地址或读回位置。
pub fn sys_arch_prctl(
    task_domain: &Arc<dyn TaskDomain>,
    code: usize,
    addr: usize,
) -> AlienResult<isize> {
    match code {
        ARCH_SET_FS => {
            task_domain.do_set_fs_base(addr)?;
            Ok(0)
        }
        ARCH_SET_GS => {
            task_domain.do_set_gs_base(addr)?;
            Ok(0)
        }
        ARCH_GET_FS => {
            if addr == 0 {
                return Err(AlienError::EFAULT);
            }
            let fs_base = task_domain.do_get_fs_base()?;
            task_domain.copy_to_user(addr, &fs_base.to_ne_bytes())?;
            Ok(0)
        }
        ARCH_GET_GS => {
            if addr == 0 {
                return Err(AlienError::EFAULT);
            }
            let gs_base = task_domain.do_get_gs_base()?;
            task_domain.copy_to_user(addr, &gs_base.to_ne_bytes())?;
            Ok(0)
        }
        _ => Err(AlienError::EINVAL),
    }
}

#[cfg(not(target_arch = "x86_64"))]
#[allow(dead_code)]
/// arch_prctl：非 x86_64 路径当前不支持，直接返回 ENOSYS。
pub fn sys_arch_prctl(
    _task_domain: &Arc<dyn TaskDomain>,
    _code: usize,
    _addr: usize,
) -> AlienResult<isize> {
    Err(AlienError::ENOSYS)
}

/// getuid：返回当前用户 ID。
pub fn sys_getuid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

/// setpgid：`pid/pgid` 已按 ABI 接入；当前最小实现不维护真实进程组，直接返回成功。
pub fn sys_set_pgid(
    task_domain: &Arc<dyn TaskDomain>,
    pid: usize,
    pgid: usize,
) -> AlienResult<isize> {
    task_domain.do_set_pgid(pid, pgid)
}

/// getpgid：返回指定进程的进程组 ID，`pid == 0` 时查询当前进程。
pub fn sys_get_pgid(task_domain: &Arc<dyn TaskDomain>, pid: usize) -> AlienResult<isize> {
    task_domain.do_get_pgid(pid).map(|pgid| pgid as isize)
}

/// getpgrp：返回当前进程组 ID。
#[cfg(target_arch = "x86_64")]
pub fn sys_get_pgrp(task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    task_domain.current_pgid().map(|pgid| pgid as isize)
}

/// getsid：返回指定进程的会话 ID，`pid == 0` 时查询当前进程。
pub fn sys_get_sid(task_domain: &Arc<dyn TaskDomain>, pid: usize) -> AlienResult<isize> {
    task_domain.do_get_sid(pid).map(|sid| sid as isize)
}

/// setsid：创建新会话并成为会话首进程。
pub fn sys_set_sid(task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    task_domain.do_set_sid()
}

/// getpid：返回当前进程 ID。
pub fn sys_get_pid(task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    task_domain.current_pid().map(|pid| pid as isize)
}

/// getppid：返回当前父进程 ID。
pub fn sys_get_ppid(task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    task_domain.current_ppid().map(|ppid| ppid as isize)
}

/// geteuid：返回当前有效用户 ID。
pub fn sys_get_euid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

/// getgid：返回当前组 ID。
pub fn sys_get_gid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

/// getegid：返回当前有效组 ID。
pub fn sys_get_egid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

/// gettid：返回当前线程 ID。
pub fn sys_get_tid() -> AlienResult<isize> {
    basic::current_tid().map(|tid| tid.unwrap() as isize)
}

/// exit：`status` 是退出码，直接结束当前任务。
pub fn sys_exit(task_domain: &Arc<dyn TaskDomain>, status: usize) -> AlienResult<isize> {
    info!("<sys_exit> status: {}", status);
    task_domain.do_exit(status as isize)
}

/// exit_group：`status` 是线程组退出码。
pub fn sys_exit_group(task_domain: &Arc<dyn TaskDomain>, status: usize) -> AlienResult<isize> {
    info!("<sys_exit_group> status: {}", status);
    task_domain.do_exit(status as isize)
}

/// setpriority：`which/who/prio` 对应 Linux 优先级接口参数。
pub fn sys_set_priority(
    task_domain: &Arc<dyn TaskDomain>,
    which: usize,
    who: usize,
    prio: usize,
) -> AlienResult<isize> {
    task_domain.do_set_priority(which as i32, who as u32, prio as i32)?;
    Ok(0)
}

/// getpriority：`which/who` 对应 Linux 优先级查询参数。
pub fn sys_get_priority(
    task_domain: &Arc<dyn TaskDomain>,
    which: usize,
    who: usize,
) -> AlienResult<isize> {
    task_domain
        .do_get_priority(which as i32, who as u32)
        .map(|prio| prio as isize)
}

/// sigaltstack：`uss` 是新栈配置，`uoss` 是旧栈输出位置。
pub fn sys_sigaltstack(task: &Arc<dyn TaskDomain>, uss: usize, uoss: usize) -> AlienResult<isize> {
    task.do_signal_stack(uss, uoss)
}

/// futex：`uaddr/uaddr2` 是用户地址，`futex_op/val/val2/val3` 是 futex 操作参数。
pub fn sys_futex(
    task_domain: &Arc<dyn TaskDomain>,
    uaddr: usize,
    futex_op: usize,
    val: usize,
    val2: usize,
    uaddr2: usize,
    val3: usize,
) -> AlienResult<isize> {
    task_domain.do_futex(
        uaddr,
        futex_op as u32,
        val as u32,
        val2,
        uaddr2,
        val3 as u32,
    )
}
