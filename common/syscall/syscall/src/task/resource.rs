use alloc::sync::Arc;

use basic::{constants::PrLimitResType, AlienError, AlienResult};
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
