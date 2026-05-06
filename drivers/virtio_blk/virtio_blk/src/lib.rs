//! This crate should implement the block device driver according to the VirtIO specification.
//! The [virtio-blk](virtio_blk) crate provides the safety abstraction for the VirtIO registers and buffers.
//! So this crate should only implement the driver logic with safe Rust code.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
use alloc::boxed::Box;
use core::{
    fmt::{Debug, Formatter},
};

use basic::{
    println,
    sync::{Mutex, Once, OnceGet},
    AlienResult,
};
use interface::{define_unwind_for_BlkDeviceDomain, Basic, BlkDeviceDomain, DeviceBase, VirtioInitInfo};
use shared_heap::DVec;
use virtio_drivers::device::block::VirtIOBlk;
use virtio_drivers::transport::DeviceType;
use virtio_common::{to_alien_err, transport_from_init_info, HalImpl, VirtioTransport};

pub struct BlkDomain {
    blk: Once<Mutex<VirtIOBlk<HalImpl, VirtioTransport>>>,
}

impl BlkDomain {
    pub fn new() -> Self {
        Self { blk: Once::new() }
    }
}

impl Debug for BlkDomain {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("BlkDomain")
    }
}

impl Basic for BlkDomain {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl DeviceBase for BlkDomain {
    fn handle_irq(&self) -> AlienResult<()> {
        self.blk
            .get_must()
            .lock()
            .ack_interrupt()
            .map_err(to_alien_err)?;
        Ok(())
    }
}

impl BlkDeviceDomain for BlkDomain {
    fn init(&self, device_info: &VirtioInitInfo) -> AlienResult<()> {
        let transport = transport_from_init_info(device_info, DeviceType::Block, "virtio_blk");
        println!("virtio_blk: create blk driver");
        let blk = VirtIOBlk::<HalImpl, VirtioTransport>::new(transport)
            .expect("failed to create virtio_blk");
        println!("virtio_blk: driver ready");
        self.blk.call_once(|| Mutex::new(blk));
        Ok(())
    }
    fn read_block(&self, block: u32, mut data: DVec<u8>) -> AlienResult<DVec<u8>> {
        #[cfg(feature = "crash")]
        if basic::blk_crash_trick() {
            panic!("blk crash trick");
        }
        self.blk
            .get_must()
            .lock()
            .read_blocks(block as _, data.as_mut_slice())
            .map_err(to_alien_err)?;
        Ok(data)
    }
    fn write_block(&self, block: u32, data: &DVec<u8>) -> AlienResult<usize> {
        self.blk
            .get_must()
            .lock()
            .write_blocks(block as _, data.as_slice())
            .map_err(to_alien_err)?;
        Ok(data.len())
    }
    fn get_capacity(&self) -> AlienResult<u64> {
        let size = self
            .blk
            .get_must()
            .lock()
            .capacity()
            .map_err(to_alien_err)?;
        Ok(size)
    }
    fn flush(&self) -> AlienResult<()> {
        self.blk
            .get_must()
            .lock()
            .flush()
            .map_err(to_alien_err)?;
        Ok(())
    }
}

define_unwind_for_BlkDeviceDomain!(BlkDomain);

pub fn main() -> Box<dyn BlkDeviceDomain> {
    Box::new(UnwindWrap::new(BlkDomain::new()))
}
