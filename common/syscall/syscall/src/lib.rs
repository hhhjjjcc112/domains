#![no_std]
#![forbid(unsafe_code)]
mod arch;
mod domain;
mod fs;
mod gui;
mod mm;
mod signal;
mod socket;
mod system;
mod task;
mod time;

extern crate alloc;
extern crate log;

use alloc::{boxed::Box, format, sync::Arc, vec, vec::Vec};

use basic::{constants::*, println, AlienError, AlienResult};
use interface::*;
use shared_heap::DVec;

#[derive(Debug)]
struct SysCallDomainImpl {
    vfs_domain: Arc<dyn VfsDomain>,
    task_domain: Arc<dyn TaskDomain>,
    logger: Arc<dyn LogDomain>,
    net_stack_domain: Option<Arc<dyn NetDomain>>,
    gpu_domain: Option<Arc<dyn GpuDomain>>,
    input_domain: Vec<Arc<dyn BufInputDomain>>,
}

impl SysCallDomainImpl {
    pub fn new(
        vfs_domain: Arc<dyn VfsDomain>,
        task_domain: Arc<dyn TaskDomain>,
        logger: Arc<dyn LogDomain>,
        net_stack_domain: Option<Arc<dyn NetDomain>>,
        gpu_domain: Option<Arc<dyn GpuDomain>>,
        input_domain: Vec<Arc<dyn BufInputDomain>>,
    ) -> Self {
        Self {
            vfs_domain,
            task_domain,
            logger,
            net_stack_domain,
            gpu_domain,
            input_domain,
        }
    }

    fn net_stack_domain(&self) -> AlienResult<Arc<dyn NetDomain>> {
        if let Some(net_stack_domain) = self.net_stack_domain.as_ref() {
            return Ok(net_stack_domain.clone());
        }
        match basic::get_domain("net_stack") {
            Some(DomainType::NetDomain(net_stack_domain)) => Ok(net_stack_domain),
            _ => Err(AlienError::ENOSYS),
        }
    }
}

impl Basic for SysCallDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl SysCallDomain for SysCallDomainImpl {
    fn init(&self) -> AlienResult<()> {
        let log_info = "syscall domain test log domain.";
        self.logger.log(
            interface::Level::Info,
            &DVec::from_slice(log_info.as_bytes()),
        )?;
        println!("syscall domain init");
        Ok(())
    }

    /// 统一接收原始 syscall，直接按子系统分发；`syscall_id` 是原始号，`args` 是 6 个原始参数。
    fn call(&self, syscall_id: usize, args: [usize; 6]) -> AlienResult<isize> {
        let tid = basic::current_tid()?;
        if syscall_id == SYSCALL_DOMAIN_TEST {
            let log_info = format!("[tid:{:?}] syscall: {}", tid, SYSCALL_DOMAIN_TEST);
            self.logger.log(
                interface::Level::Info,
                &DVec::from_slice(log_info.as_bytes()),
            )?;
            return Ok(0);
        }
        arch::dispatch(self, syscall_id, args)
    }
}
define_unwind_for_SysCallDomain!(SysCallDomainImpl);

pub fn main() -> Box<dyn SysCallDomain> {
    let vfs_domain = basic::get_domain("vfs").unwrap();
    let vfs_domain = match vfs_domain {
        DomainType::VfsDomain(vfs_domain) => vfs_domain,
        _ => panic!("vfs domain not found"),
    };
    let task_domain = basic::get_domain("task").unwrap();
    let task_domain = match task_domain {
        DomainType::TaskDomain(task_domain) => task_domain,
        _ => panic!("task domain not found"),
    };

    let logger = basic::get_domain("logger").unwrap();
    let logger = match logger {
        DomainType::LogDomain(logger) => logger,
        _ => panic!("logger domain not found"),
    };

    let net_stack_domain = match basic::get_domain("net_stack") {
        Some(DomainType::NetDomain(net_stack_domain)) => Some(net_stack_domain),
        Some(_) => {
            log::warn!("net_stack domain type mismatch, skip");
            None
        }
        None => {
            log::warn!("net_stack domain not found, skip");
            None
        }
    };

    let mut gpu_domain = None;
    for name in ["gpu", "gpu-1", "virtio_gpu-1", "virtio_gpu"] {
        if let Some(DomainType::GpuDomain(domain)) = basic::get_domain(name) {
            gpu_domain = Some(domain);
            break;
        }
    }

    let mut input_domains = vec![];
    let mut count = 1;
    loop {
        let name = format!("buf_input-{}", count);
        let buf_input_domain = basic::get_domain(&name);
        match buf_input_domain {
            Some(DomainType::BufInputDomain(buf_input_domain)) => {
                input_domains.push(buf_input_domain);
                count += 1;
            }
            _ => {
                break;
            }
        }
    }
    println!("syscall get {} input domain", count - 1);
    Box::new(UnwindWrap::new(SysCallDomainImpl::new(
        vfs_domain,
        task_domain,
        logger,
        net_stack_domain,
        gpu_domain,
        input_domains,
    )))
}
