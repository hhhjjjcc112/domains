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

    match syscall_id {
        // riscv64 clone 顺序本身就与内核实现一致，无需兼容重排。
        SYSCALL_CLONE => sys_clone(task, args[0], args[1], args[2], args[3], args[4]),
        _ => dispatch_rest(domain, syscall_id, args),
    }
}
