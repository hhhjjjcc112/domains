#![no_std]
#![no_main]
#![feature(lang_items)]
#![allow(internal_features)]
extern crate alloc;
extern crate malloc;
use alloc::boxed::Box;
use core::panic::PanicInfo;

use basic::domain_main;
use corelib::CoreFunction;
use interface::{APICDomain, Basic};
use shared_heap::{DVec, SharedHeapAlloc};
use storage::StorageArg;

#[domain_main]
fn main(
    sys: &'static dyn CoreFunction,
    domain_id: u64,
    shared_heap: &'static dyn SharedHeapAlloc,
    storage_arg: StorageArg,
) -> Box<dyn APICDomain> {
    corelib::init(sys);
    shared_heap::init(shared_heap, domain_id);
    basic::logging::init_logger();
    let StorageArg { allocator, storage } = storage_arg;
    storage::init_database(storage);
    storage::init_data_allocator(allocator);
    interface::activate_domain();
    domain_entry()
}

#[cfg(target_arch = "riscv64")]
fn domain_entry() -> Box<dyn APICDomain> {
    apic::main()
}

#[cfg(target_arch = "x86_64")]
fn domain_entry() -> Box<dyn APICDomain> {
    Box::new(ApicStub)
}

#[cfg(target_arch = "x86_64")]
#[derive(Debug, Default)]
struct ApicStub;

#[cfg(target_arch = "x86_64")]
impl Basic for ApicStub {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

#[cfg(target_arch = "x86_64")]
impl APICDomain for ApicStub {
    fn init(&self) -> basic::AlienResult<()> {
        Ok(())
    }

    fn handle_irq(&self, _irq: usize) -> basic::AlienResult<()> {
        Ok(())
    }

    fn register_irq(&self, _irq: usize, _device_domain_name: &DVec<u8>) -> basic::AlienResult<()> {
        Ok(())
    }

    fn irq_info(&self, buf: DVec<u8>) -> basic::AlienResult<DVec<u8>> {
        Ok(buf)
    }
}
