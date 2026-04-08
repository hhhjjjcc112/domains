mod ipc;
mod resource;

use alloc::sync::Arc;

use basic::{constants::*, AlienError, AlienResult};
use interface::TaskDomain;
pub use ipc::*;
use log::info;
pub use resource::*;

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

pub fn sys_wait4(
    task_domain: &Arc<dyn TaskDomain>,
    pid: usize,
    status: usize,
    options: usize,
    rusage: usize,
) -> AlienResult<isize> {
    task_domain.do_wait4(pid as isize, status, options as u32, rusage)
}

pub fn sys_waitid(
    task_domain: &Arc<dyn TaskDomain>,
    which: usize,
    pid: usize,
    infop: usize,
    options: usize,
    rusage: usize,
) -> AlienResult<isize> {
    const P_ALL: usize = 0;
    const P_PID: usize = 1;
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

    let wait_pid = match which {
        P_ALL => -1,
        P_PID => pid as isize,
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

pub fn sys_execve(
    task_domain: &Arc<dyn TaskDomain>,
    filename_ptr: usize,
    argv_ptr: usize,
    envp_ptr: usize,
) -> AlienResult<isize> {
    task_domain.do_execve(filename_ptr, argv_ptr, envp_ptr)
}

pub fn sys_yield() -> AlienResult<isize> {
    basic::yield_now()?;
    Ok(0)
}

pub fn sys_set_tid_address(task_domain: &Arc<dyn TaskDomain>, tidptr: usize) -> AlienResult<isize> {
    task_domain.do_set_tid_address(tidptr)
}

#[cfg(target_arch = "x86_64")]
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
pub fn sys_arch_prctl(
    _task_domain: &Arc<dyn TaskDomain>,
    _code: usize,
    _addr: usize,
) -> AlienResult<isize> {
    Err(AlienError::ENOSYS)
}

pub fn sys_getuid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

pub fn sys_set_pgid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

pub fn sys_get_pgid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

pub fn sys_set_sid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

pub fn sys_get_pid(task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    task_domain.current_pid().map(|pid| pid as isize)
}

pub fn sys_get_ppid(task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    task_domain.current_ppid().map(|ppid| ppid as isize)
}

pub fn sys_get_euid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

pub fn sys_get_gid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

pub fn sys_get_egid(_task_domain: &Arc<dyn TaskDomain>) -> AlienResult<isize> {
    Ok(0)
}

pub fn sys_get_tid() -> AlienResult<isize> {
    basic::current_tid().map(|tid| tid.unwrap() as isize)
}

pub fn sys_exit(task_domain: &Arc<dyn TaskDomain>, status: usize) -> AlienResult<isize> {
    info!("<sys_exit> status: {}", status);
    task_domain.do_exit(status as isize)
}

pub fn sys_exit_group(task_domain: &Arc<dyn TaskDomain>, status: usize) -> AlienResult<isize> {
    info!("<sys_exit_group> status: {}", status);
    task_domain.do_exit(status as isize)
}

pub fn sys_set_priority(
    task_domain: &Arc<dyn TaskDomain>,
    which: usize,
    who: usize,
    prio: usize,
) -> AlienResult<isize> {
    task_domain.do_set_priority(which as i32, who as u32, prio as i32)?;
    Ok(0)
}

pub fn sys_get_priority(
    task_domain: &Arc<dyn TaskDomain>,
    which: usize,
    who: usize,
) -> AlienResult<isize> {
    task_domain
        .do_get_priority(which as i32, who as u32)
        .map(|prio| prio as isize)
}

/// See https://man7.org/linux/man-pages/man2/sigaltstack.2.html
pub fn sys_sigaltstack(task: &Arc<dyn TaskDomain>, uss: usize, uoss: usize) -> AlienResult<isize> {
    task.do_signal_stack(uss, uoss)
}

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
