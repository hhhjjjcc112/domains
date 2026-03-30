#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::boxed::Box;
use core::fmt::Debug;

use basic::AlienResult;
#[cfg(target_arch = "riscv64")]
use interface::define_unwind_for_EmptyDeviceDomain;
use interface::{Basic, EmptyDeviceDomain};
use shared_heap::DVec;

#[derive(Debug)]
pub struct IoApicDomainImpl;

impl Basic for IoApicDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl EmptyDeviceDomain for IoApicDomainImpl {
    fn init(&self) -> AlienResult<()> {
        Ok(())
    }

    fn read(&self, mut data: DVec<u8>) -> AlienResult<DVec<u8>> {
        data.as_mut_slice().fill(0);
        Ok(data)
    }

    fn write(&self, data: &DVec<u8>) -> AlienResult<usize> {
        Ok(data.len())
    }
}

#[cfg(target_arch = "riscv64")]
define_unwind_for_EmptyDeviceDomain!(IoApicDomainImpl);

pub fn main() -> Box<dyn EmptyDeviceDomain> {
    #[cfg(target_arch = "riscv64")]
    {
        Box::new(UnwindWrap::new(IoApicDomainImpl))
    }
    #[cfg(target_arch = "x86_64")]
    {
        Box::new(IoApicDomainImpl)
    }
}
