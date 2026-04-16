use alloc::sync::Arc;

use basic::{constants::{PrLimitResType, sys::{Rusage, RusageFlag}}, AlienError, AlienResult};
use interface::TaskDomain;

/// prlimit64：`pid/resource/new_limit/old_limit` 对应 Linux 资源限制接口参数。
pub fn sys_prlimit64(
    task_domain: &Arc<dyn TaskDomain>,
    pid: usize,
    resource: usize,
    new_limit: usize,
    old_limit: usize,
) -> AlienResult<isize> {
    PrLimitResType::try_from(resource).map_err(|_| AlienError::EINVAL)?;
    task_domain.do_prlimit(pid, resource, new_limit, old_limit)
}

/// getrlimit：读取当前任务的资源限制。
pub fn sys_getrlimit(
    task_domain: &Arc<dyn TaskDomain>,
    resource: usize,
    old_limit: usize,
) -> AlienResult<isize> {
    PrLimitResType::try_from(resource).map_err(|_| AlienError::EINVAL)?;
    if old_limit == 0 {
        return Err(AlienError::EFAULT);
    }
    task_domain.do_prlimit(0, resource, 0, old_limit)
}

/// setrlimit：设置当前任务的资源限制。
pub fn sys_setrlimit(
    task_domain: &Arc<dyn TaskDomain>,
    resource: usize,
    new_limit: usize,
) -> AlienResult<isize> {
    PrLimitResType::try_from(resource).map_err(|_| AlienError::EINVAL)?;
    if new_limit == 0 {
        return Err(AlienError::EFAULT);
    }
    task_domain.do_prlimit(0, resource, new_limit, 0)
}

/// getrusage：当前以最小实现返回零值统计，先满足常见调用路径。
pub fn sys_getrusage(
    task_domain: &Arc<dyn TaskDomain>,
    who: usize,
    usage: usize,
) -> AlienResult<isize> {
    RusageFlag::try_from(who as isize).map_err(|_| AlienError::EINVAL)?;
    if usage == 0 {
        return Err(AlienError::EFAULT);
    }
    let rusage = Rusage::default();
    task_domain.write_val_to_user(usage, &rusage)?;
    Ok(0)
}

/// madvise：`addr/len/advice` 对应用户态内存建议参数；当前仅保留最小接口。
pub fn sys_madvise(
    _task_domain: &Arc<dyn TaskDomain>,
    _addr: usize,
    _len: usize,
    _advice: usize,
) -> AlienResult<isize> {
    // task_domain.do_madvise(addr, len, advice)
    Ok(0)
}
