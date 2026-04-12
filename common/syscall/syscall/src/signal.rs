use alloc::sync::Arc;

use basic::{
    constants::signal::{SigProcMaskHow, SignalNumber},
    AlienError, AlienResult,
};
use interface::TaskDomain;
use log::info;
use pconst::signal::SIGSET_SIZE_IN_BYTE;

/// sigaction：`sig` 是信号号，`action/old_action` 是用户态结构体指针，`sig_set_size` 是信号集大小。
pub fn sys_sigaction(
    task_domain: &Arc<dyn TaskDomain>,
    sig: usize,
    action: usize,
    old_action: usize,
    sig_set_size: usize,
) -> AlienResult<isize> {
    // let action = action as *const SigAction;
    // let old_action = old_action as *mut SigAction;
    // check whether sig is valid
    let signum = SignalNumber::try_from(sig as u8).map_err(|_| AlienError::EINVAL)?;
    if signum == SignalNumber::SIGSTOP
        || signum == SignalNumber::SIGKILL
        || signum == SignalNumber::ERR
    {
        return Err(AlienError::EINVAL);
    }
    if sig_set_size != SIGSET_SIZE_IN_BYTE {
        return Err(AlienError::EINVAL);
    }
    let res = task_domain.do_sigaction(sig as _, action, old_action);
    info!("<sys_sigaction> res: {:?}", res);
    res
}

/// sigprocmask：`how` 是操作方式，`set/oldset` 是信号集合指针，`sig_set_size` 保留给 ABI 使用。
pub fn sys_sigprocmask(
    task_domain: &Arc<dyn TaskDomain>,
    how: usize,
    set: usize,
    oldset: usize,
    _sig_set_size: usize,
) -> AlienResult<isize> {
    let how = SigProcMaskHow::try_from(how).map_err(|_| AlienError::EINVAL)?;
    task_domain.do_sigprocmask(how as _, set, oldset)
}
