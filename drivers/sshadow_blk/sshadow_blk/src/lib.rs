#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::{
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
};
use core::sync::atomic::AtomicBool;

use basic::{
    println_color,
    sync::{Once, OnceGet},
    AlienError, AlienResult,
};
use interface::*;
use log::error;
use shared_heap::DVec;

#[derive(Debug)]
pub struct ShadowBlockDomainImpl {
    blk_domain_name: Once<String>,
    blk: Once<Arc<dyn BlkDeviceDomain>>,
}

impl Default for ShadowBlockDomainImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl ShadowBlockDomainImpl {
    pub fn new() -> Self {
        Self {
            blk_domain_name: Once::new(),
            blk: Once::new(),
        }
    }
}
impl Basic for ShadowBlockDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl DeviceBase for ShadowBlockDomainImpl {
    fn handle_irq(&self) -> AlienResult<()> {
        self.blk.get_must().handle_irq()
    }
}

impl ShadowBlockDomain for ShadowBlockDomainImpl {
    fn init(&self, blk_domain: &str) -> AlienResult<()> {
        let blk = basic::get_domain(blk_domain).unwrap();
        let blk = match blk {
            DomainType::BlkDeviceDomain(blk) => blk,
            _ => panic!("not a block domain"),
        };
        self.blk_domain_name.call_once(|| blk_domain.to_string());
        self.blk.call_once(|| blk);
        Ok(())
    }

    fn read_block(&self, block: u32, data: DVec<u8>) -> AlienResult<DVec<u8>> {
        static FLAG: AtomicBool = AtomicBool::new(false);
        if !FLAG.load(core::sync::atomic::Ordering::Relaxed) {
            println_color!(34, "<SShadowBlockDomainImpl Mask> read block: {}", block);
            FLAG.store(true, core::sync::atomic::Ordering::Relaxed);
        }
        let blk = self.blk.get_must();
        let mut data = data;
        let res = blk.read_block(block, data);
        match res {
            Ok(res) => Ok(res),
            Err(AlienError::DOMAINCRASH) => {
                error!("domain crash, try restart domain");
                // try reread block
                basic::checkout_shared_data().unwrap();
                println_color!(31, "try reread block");
                data = DVec::<u8>::new_uninit(512);
                blk.read_block(block, data)
            }
            Err(e) => Err(e),
        }
    }

    fn write_block(&self, block: u32, data: &DVec<u8>) -> AlienResult<usize> {
        self.blk.get_must().write_block(block, data)
    }

    fn get_capacity(&self) -> AlienResult<u64> {
        self.blk.get_must().get_capacity()
    }

    fn flush(&self) -> AlienResult<()> {
        self.blk.get_must().flush()
    }
}
define_unwind_for_ShadowBlockDomain!(ShadowBlockDomainImpl);

pub fn main() -> Box<dyn ShadowBlockDomain> {
    Box::new(UnwindWrap::new(ShadowBlockDomainImpl::new()))
}
