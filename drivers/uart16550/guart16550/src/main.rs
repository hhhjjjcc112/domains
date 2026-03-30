#![no_std]
#![no_main]
#![feature(lang_items)]
#![allow(internal_features)]
extern crate alloc;
extern crate malloc;
use alloc::boxed::Box;
use core::ops::Range;
use core::panic::PanicInfo;

use basic::domain_main;
use corelib::CoreFunction;
use interface::{Basic, DeviceBase, UartDomain};
use shared_heap::{DVec, SharedHeapAlloc};
use storage::StorageArg;

#[domain_main]
fn main(
    sys: &'static dyn CoreFunction,
    domain_id: u64,
    shared_heap: &'static dyn SharedHeapAlloc,
    storage_arg: StorageArg,
) -> Box<dyn UartDomain> {
    // init basic
    corelib::init(sys);
    // init shared_heap's shared heap
    shared_heap::init(shared_heap, domain_id);
    basic::logging::init_logger();
    // init storage
    let StorageArg { allocator, storage } = storage_arg;
    storage::init_database(storage);
    storage::init_data_allocator(allocator);
    // activate the domain
    interface::activate_domain();
    domain_entry()
}

#[cfg(target_arch = "riscv64")]
fn domain_entry() -> Box<dyn UartDomain> {
    uart16550::main()
}

#[cfg(target_arch = "x86_64")]
fn domain_entry() -> Box<dyn UartDomain> {
    Box::new(UartStub)
}

#[cfg(target_arch = "x86_64")]
#[derive(Debug, Default)]
struct UartStub;

#[cfg(target_arch = "x86_64")]
impl Basic for UartStub {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

#[cfg(target_arch = "x86_64")]
impl DeviceBase for UartStub {
    fn handle_irq(&self) -> basic::AlienResult<()> {
        Ok(())
    }
}

#[cfg(target_arch = "x86_64")]
impl UartDomain for UartStub {
    fn init(&self, _device_info: &Range<usize>) -> basic::AlienResult<()> {
        Ok(())
    }

    fn putc(&self, _ch: u8) -> basic::AlienResult<()> {
        Ok(())
    }

    fn getc(&self) -> basic::AlienResult<Option<u8>> {
        Ok(None)
    }

    fn put_bytes(&self, buf: &DVec<u8>) -> basic::AlienResult<usize> {
        Ok(buf.len())
    }

    fn have_data_to_get(&self) -> basic::AlienResult<bool> {
        Ok(false)
    }

    fn enable_receive_interrupt(&self) -> basic::AlienResult<()> {
        Ok(())
    }

    fn disable_receive_interrupt(&self) -> basic::AlienResult<()> {
        Ok(())
    }
}
