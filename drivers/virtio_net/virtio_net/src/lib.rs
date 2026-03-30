#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use core::{
    fmt::{Debug, Formatter, Result},
};

use basic::{
    io::SafeIORegion,
    println,
    sync::{Mutex, Once, OnceGet},
    AlienResult,
};
use interface::{define_unwind_for_NetDeviceDomain, Basic, DeviceBase, NetDeviceDomain, VirtioInitInfo};
use shared_heap::DVec;
use virtio_drivers::error::VirtIoResult;
use virtio_drivers::hal::VirtIoDeviceIo;
use virtio_drivers::transport::{DeviceStatus, DeviceType, Transport};
use virtio_drivers::transport::mmio::MmioTransport;
use virtio_drivers::transport::pci::{LegacyPciTransport, ModernPciTransport};
use virtio_drivers::device::net::VirtIONet;
use virtio_mmio_common::{to_alien_err, HalImpl, SafeIORW};

pub const NET_QUEUE_SIZE: usize = 128;
pub const NET_BUF_LEN: usize = 4096;

#[derive(Default)]
pub struct VirtIoNetDomain {
    nic: Once<Mutex<VirtIONet<HalImpl, VirtioTransport, NET_QUEUE_SIZE>>>,
}

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

impl Debug for VirtIoNetDomain {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        writeln!(f, "NicDomain")
    }
}

impl Basic for VirtIoNetDomain {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl DeviceBase for VirtIoNetDomain {
    fn handle_irq(&self) -> AlienResult<()> {
        log::info!("<VirtIoNetDomain as DeviceBase>::handle_irq() called");
        self.nic
            .get_must()
            .lock()
            .ack_interrupt()
            .map_err(to_alien_err)?;
        Ok(())
    }
}

pub const NET_BUFFER_LEN: usize = 1600;

impl NetDeviceDomain for VirtIoNetDomain {
    fn init(&self, init_info: &VirtioInitInfo) -> AlienResult<()> {
        let transport = match init_info {
            VirtioInitInfo::Mmio { range, .. } => {
                println!("virtio_net_mmio: {:#x}-{:#x}", range.start, range.end);
                let io_region = SafeIORW(SafeIORegion::from(range.clone()));
                VirtioTransport::Mmio(MmioTransport::new(Box::new(io_region)).unwrap())
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
                if let (
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
                        "virtio_net_pci(modern): {:04x}:{:02x}:{:02x}.{}",
                        segment, bus, device, function
                    );
                    VirtioTransport::PciModern(
                        ModernPciTransport::new(
                            Box::new(SafeIORW(SafeIORegion::from(common))),
                            Box::new(SafeIORW(SafeIORegion::from(notify))),
                            Box::new(SafeIORW(SafeIORegion::from(isr))),
                            Box::new(SafeIORW(SafeIORegion::from(device_cfg))),
                            notify_mul,
                            DeviceType::Network,
                        )
                        .unwrap(),
                    )
                } else if let Some(io_range) = legacy_io.clone() {
                    println!(
                        "virtio_net_pci(legacy): {:04x}:{:02x}:{:02x}.{}, io={:#x}-{:#x}",
                        segment, bus, device, function, io_range.start, io_range.end
                    );
                    let io_region = SafeIORW(SafeIORegion::from(io_range));
                    VirtioTransport::PciLegacy(
                        LegacyPciTransport::new(Box::new(io_region), DeviceType::Network).unwrap(),
                    )
                } else {
                    panic!("virtio-net pci has no usable transport info");
                }
            }
        };
        let net = VirtIONet::new(transport, NET_BUFFER_LEN).expect("failed to create virtio net");
        self.nic.call_once(|| Mutex::new(net));
        Ok(())
    }

    fn mac_address(&self) -> AlienResult<[u8; 6]> {
        self.nic
            .get_must()
            .lock()
            .mac_address()
            .map_err(to_alien_err)
    }

    fn can_transmit(&self) -> AlienResult<bool> {
        self.nic.get_must().lock().can_send().map_err(to_alien_err)
    }

    fn can_receive(&self) -> AlienResult<bool> {
        Ok(self
            .nic
            .get_must()
            .lock()
            .can_recv()
            .map_err(to_alien_err)?
            .is_some())
    }

    fn rx_queue_size(&self) -> AlienResult<usize> {
        Ok(NET_QUEUE_SIZE)
    }

    fn tx_queue_size(&self) -> AlienResult<usize> {
        Ok(NET_QUEUE_SIZE)
    }

    fn transmit(&self, tx_buf: &DVec<u8>) -> AlienResult<()> {
        self.nic
            .get_must()
            .lock()
            .send(tx_buf.as_slice())
            .map_err(to_alien_err)
    }

    fn receive(&self, mut rx_buf: DVec<u8>) -> AlienResult<(DVec<u8>, usize)> {
        let len = self
            .nic
            .get_must()
            .lock()
            .receive(rx_buf.as_mut_slice())
            .map_err(to_alien_err)?;
        Ok((rx_buf, len))
    }
}

define_unwind_for_NetDeviceDomain!(VirtIoNetDomain);

pub fn main() -> Box<dyn NetDeviceDomain> {
    Box::new(UnwindWrap::new(VirtIoNetDomain::default()))
}
