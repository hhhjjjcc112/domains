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
use interface::RtcDomain;
use shared_heap::SharedHeapAlloc;
use storage::StorageArg;

#[domain_main]
fn main(
    sys: &'static dyn CoreFunction,
    domain_id: u64,
    shared_heap: &'static dyn SharedHeapAlloc,
    storage_arg: StorageArg,
) -> Box<dyn RtcDomain> {
    // 初始化基础运行时。
    corelib::init(sys);
    shared_heap::init(shared_heap, domain_id);
    basic::logging::init_logger();

    // 初始化存储与共享分配器。
    let StorageArg { allocator, storage } = storage_arg;
    storage::init_database(storage);
    storage::init_data_allocator(allocator);

    interface::activate_domain();
    cmos_rtc::main()
}
