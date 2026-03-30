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
use interface::{Basic, EmptyDeviceDomain};
use shared_heap::{DVec, SharedHeapAlloc};
use storage::StorageArg;

#[domain_main]
fn main(
    sys: &'static dyn CoreFunction,
    domain_id: u64,
    shared_heap: &'static dyn SharedHeapAlloc,
    storage_arg: StorageArg,
) -> Box<dyn EmptyDeviceDomain> {
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
fn domain_entry() -> Box<dyn EmptyDeviceDomain> {
    io_apic::main()
}

#[cfg(target_arch = "x86_64")]
fn domain_entry() -> Box<dyn EmptyDeviceDomain> {
    Box::new(IoApicStub)
}

#[cfg(target_arch = "x86_64")]
#[derive(Debug, Default)]
struct IoApicStub;

#[cfg(target_arch = "x86_64")]
impl Basic for IoApicStub {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

#[cfg(target_arch = "x86_64")]
impl EmptyDeviceDomain for IoApicStub {
    fn init(&self) -> basic::AlienResult<()> {
        Ok(())
    }

    fn read(&self, mut data: DVec<u8>) -> basic::AlienResult<DVec<u8>> {
        data.as_mut_slice().fill(0);
        Ok(data)
    }

    fn write(&self, data: &DVec<u8>) -> basic::AlienResult<usize> {
        Ok(data.len())
    }
}
