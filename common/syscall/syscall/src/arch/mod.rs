use basic::{AlienError, AlienResult};

use super::SysCallDomainImpl;

mod linux_common;
mod private;
#[cfg(target_arch = "riscv64")]
mod riscv64;
#[cfg(target_arch = "x86_64")]
mod x86_64;

pub(super) fn dispatch(
    domain: &SysCallDomainImpl,
    syscall_id: usize,
    args: [usize; 6],
) -> AlienResult<isize> {
    #[cfg(target_arch = "x86_64")]
    {
        return x86_64::dispatch(domain, syscall_id, args);
    }

    #[cfg(target_arch = "riscv64")]
    {
        return riscv64::dispatch(domain, syscall_id, args);
    }

    #[allow(unreachable_code)]
    Err(AlienError::ENOSYS)
}

pub(super) fn dispatch_rest(
    domain: &SysCallDomainImpl,
    syscall_id: usize,
    args: [usize; 6],
) -> AlienResult<isize> {
    if let Some(result) = linux_common::dispatch(domain, syscall_id, args) {
        return result;
    }
    if let Some(result) = private::dispatch(domain, syscall_id, args) {
        return result;
    }
    log::warn!("unsupported syscall raw={:#x}", syscall_id);
    Err(AlienError::ENOSYS)
}
