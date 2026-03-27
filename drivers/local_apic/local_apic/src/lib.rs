#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::boxed::Box;
use core::fmt::Debug;

use basic::AlienResult;
use interface::{define_unwind_for_EmptyDeviceDomain, Basic, EmptyDeviceDomain};
use shared_heap::DVec;

#[derive(Debug)]
pub struct LocalApicDomainImpl;

impl Basic for LocalApicDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl EmptyDeviceDomain for LocalApicDomainImpl {
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

define_unwind_for_EmptyDeviceDomain!(LocalApicDomainImpl);

pub fn main() -> Box<dyn EmptyDeviceDomain> {
    Box::new(UnwindWrap::new(LocalApicDomainImpl))
}
