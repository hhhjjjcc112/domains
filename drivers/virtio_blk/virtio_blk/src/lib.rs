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
    io::SafeIORegion,
    println,
    sync::{Mutex, Once, OnceGet},
    AlienResult,
};
use interface::{define_unwind_for_BlkDeviceDomain, Basic, BlkDeviceDomain, DeviceBase, VirtioInitInfo};
use shared_heap::{DBox, DVec};
use virtio_drivers::error::VirtIoResult;
use virtio_drivers::hal::VirtIoDeviceIo;
use virtio_drivers::transport::{DeviceStatus, DeviceType, Transport};
use virtio_drivers::transport::mmio::MmioTransport;
use virtio_drivers::transport::pci::{LegacyPciTransport, ModernPciTransport};
use virtio_drivers::device::block::VirtIOBlk;
use virtio_mmio_common::{to_alien_err, HalImpl, SafeIORW};

pub enum VirtioTransport {
    Mmio(MmioTransport),
    PciLegacy(LegacyPciTransport),
    PciModern(ModernPciTransport),
}

impl Transport for VirtioTransport {
    fn device_type(&self) -> VirtIoResult<DeviceType> {
        match self {
            Self::Mmio(inner) => inner.device_type(),
            Self::PciLegacy(inner) => inner.device_type(),
            Self::PciModern(inner) => inner.device_type(),
        }
    }

    fn read_device_features(&mut self) -> VirtIoResult<u64> {
        match self {
            Self::Mmio(inner) => inner.read_device_features(),
            Self::PciLegacy(inner) => inner.read_device_features(),
            Self::PciModern(inner) => inner.read_device_features(),
        }
    }

    fn write_driver_features(&mut self, driver_features: u64) -> VirtIoResult<()> {
        match self {
            Self::Mmio(inner) => inner.write_driver_features(driver_features),
            Self::PciLegacy(inner) => inner.write_driver_features(driver_features),
            Self::PciModern(inner) => inner.write_driver_features(driver_features),
        }
    }

    fn max_queue_size(&mut self, queue: u16) -> VirtIoResult<u32> {
        match self {
            Self::Mmio(inner) => inner.max_queue_size(queue),
            Self::PciLegacy(inner) => inner.max_queue_size(queue),
            Self::PciModern(inner) => inner.max_queue_size(queue),
        }
    }

    fn notify(&mut self, queue: u16) -> VirtIoResult<()> {
        match self {
            Self::Mmio(inner) => inner.notify(queue),
            Self::PciLegacy(inner) => inner.notify(queue),
            Self::PciModern(inner) => inner.notify(queue),
        }
    }

    fn get_status(&self) -> VirtIoResult<DeviceStatus> {
        match self {
            Self::Mmio(inner) => inner.get_status(),
            Self::PciLegacy(inner) => inner.get_status(),
            Self::PciModern(inner) => inner.get_status(),
        }
    }

    fn set_status(&mut self, status: DeviceStatus) -> VirtIoResult<()> {
        match self {
            Self::Mmio(inner) => inner.set_status(status),
            Self::PciLegacy(inner) => inner.set_status(status),
            Self::PciModern(inner) => inner.set_status(status),
        }
    }

    fn set_guest_page_size(&mut self, guest_page_size: u32) -> VirtIoResult<()> {
        match self {
            Self::Mmio(inner) => inner.set_guest_page_size(guest_page_size),
            Self::PciLegacy(inner) => inner.set_guest_page_size(guest_page_size),
            Self::PciModern(inner) => inner.set_guest_page_size(guest_page_size),
        }
    }

    fn requires_legacy_layout(&self) -> bool {
        match self {
            Self::Mmio(inner) => inner.requires_legacy_layout(),
            Self::PciLegacy(inner) => inner.requires_legacy_layout(),
            Self::PciModern(inner) => inner.requires_legacy_layout(),
        }
    }

    fn queue_set(
        &mut self,
        queue: u16,
        size: u32,
        descriptors: usize,
        driver_area: usize,
        device_area: usize,
    ) -> VirtIoResult<()> {
        match self {
            Self::Mmio(inner) => inner.queue_set(queue, size, descriptors, driver_area, device_area),
            Self::PciLegacy(inner) => inner.queue_set(queue, size, descriptors, driver_area, device_area),
            Self::PciModern(inner) => inner.queue_set(queue, size, descriptors, driver_area, device_area),
        }
    }

    fn queue_unset(&mut self, queue: u16) -> VirtIoResult<()> {
        match self {
            Self::Mmio(inner) => inner.queue_unset(queue),
            Self::PciLegacy(inner) => inner.queue_unset(queue),
            Self::PciModern(inner) => inner.queue_unset(queue),
        }
    }

    fn queue_used(&mut self, queue: u16) -> VirtIoResult<bool> {
        match self {
            Self::Mmio(inner) => inner.queue_used(queue),
            Self::PciLegacy(inner) => inner.queue_used(queue),
            Self::PciModern(inner) => inner.queue_used(queue),
        }
    }

    fn ack_interrupt(&mut self) -> VirtIoResult<bool> {
        match self {
            Self::Mmio(inner) => inner.ack_interrupt(),
            Self::PciLegacy(inner) => inner.ack_interrupt(),
            Self::PciModern(inner) => inner.ack_interrupt(),
        }
    }

    fn io_region(&self) -> &dyn VirtIoDeviceIo {
        match self {
            Self::Mmio(inner) => inner.io_region(),
            Self::PciLegacy(inner) => inner.io_region(),
            Self::PciModern(inner) => inner.io_region(),
        }
    }
}

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
        match device_info {
            VirtioInitInfo::Mmio { range, .. } => {
                println!("virtio_blk_mmio: {:#x}-{:#x}", range.start, range.end);
                let io_region = SafeIORW(SafeIORegion::from(range.clone()));
                let transport = VirtioTransport::Mmio(MmioTransport::new(Box::new(io_region)).unwrap());
                let blk = VirtIOBlk::<HalImpl, VirtioTransport>::new(transport)
                    .expect("failed to create virtio_blk");
                self.blk.call_once(|| Mutex::new(blk));
            }
            VirtioInitInfo::Pci {
                segment,
                bus,
                device,
                function,
                legacy_io,
                modern_common,
                modern_notify,
                modern_notify_off_multiplier,
                modern_isr,
                modern_device,
                ..
            } => {
                let transport = if let (
                    Some(common),
                    Some(notify),
                    Some(notify_mul),
                    Some(isr),
                    Some(device_cfg),
                ) = (
                    modern_common.clone(),
                    modern_notify.clone(),
                    *modern_notify_off_multiplier,
                    modern_isr.clone(),
                    modern_device.clone(),
                ) {
                    println!(
                        "virtio_blk_pci(modern): {:04x}:{:02x}:{:02x}.{}",
                        segment, bus, device, function
                    );
                    VirtioTransport::PciModern(
                        ModernPciTransport::new(
                            Box::new(SafeIORW(SafeIORegion::from(common))),
                            Box::new(SafeIORW(SafeIORegion::from(notify))),
                            Box::new(SafeIORW(SafeIORegion::from(isr))),
                            Box::new(SafeIORW(SafeIORegion::from(device_cfg))),
                            notify_mul,
                            DeviceType::Block,
                        )
                        .unwrap(),
                    )
                } else if let Some(io_range) = legacy_io.clone() {
                    println!(
                        "virtio_blk_pci(legacy): {:04x}:{:02x}:{:02x}.{}, io={:#x}-{:#x}",
                        segment, bus, device, function, io_range.start, io_range.end
                    );
                    let io_region = SafeIORW(SafeIORegion::from(io_range));
                    VirtioTransport::PciLegacy(
                        LegacyPciTransport::new(Box::new(io_region), DeviceType::Block).unwrap(),
                    )
                } else {
                    panic!("virtio-blk pci has no usable transport info");
                };
                println!("virtio_blk_pci: create blk driver");
                let blk = VirtIOBlk::<HalImpl, VirtioTransport>::new(transport)
                    .expect("failed to create virtio_blk from pci");
                println!("virtio_blk_pci: driver ready");
                self.blk.call_once(|| Mutex::new(blk));
            }
        }
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
