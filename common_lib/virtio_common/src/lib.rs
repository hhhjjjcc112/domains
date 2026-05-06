#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::boxed::Box;

use basic::{io::SafeIORegion, vm::frame::FrameTracker, AlienError};
use interface::VirtioInitInfo;
use virtio_drivers::{
    transport::{
        mmio::MmioTransport,
        pci::{LegacyPciTransport, ModernPciTransport},
        DeviceStatus, DeviceType, Transport,
    },
    error::{VirtIoError, VirtIoResult},
    hal::{DevicePage, Hal, QueuePage, VirtIoDeviceIo},
    queue::{QueueLayout, QueueMutRef},
    PhysAddr, VirtAddr,
};

#[derive(Debug)]
pub struct SafeIORW(pub SafeIORegion);

impl VirtIoDeviceIo for SafeIORW {
    fn read_volatile_u32_at(&self, off: usize) -> VirtIoResult<u32> {
        self.0.read_at(off).map_err(|_| VirtIoError::IoError)
    }

    fn read_volatile_u16_at(&self, off: usize) -> VirtIoResult<u16> {
        self.0.read_at(off).map_err(|_| VirtIoError::IoError)
    }

    fn read_volatile_u8_at(&self, off: usize) -> VirtIoResult<u8> {
        self.0.read_at(off).map_err(|_| VirtIoError::IoError)
    }

    fn write_volatile_u32_at(&self, off: usize, data: u32) -> VirtIoResult<()> {
        self.0.write_at(off, data).map_err(|_| VirtIoError::IoError)
    }

    fn write_volatile_u16_at(&self, off: usize, data: u16) -> VirtIoResult<()> {
        self.0.write_at(off, data).map_err(|_| VirtIoError::IoError)
    }

    fn write_volatile_u8_at(&self, off: usize, data: u8) -> VirtIoResult<()> {
        self.0.write_at(off, data).map_err(|_| VirtIoError::IoError)
    }

    fn paddr(&self) -> PhysAddr {
        self.0.phys_addr().as_usize()
    }

    fn vaddr(&self) -> VirtAddr {
        self.0.virt_addr().as_usize()
    }
}

pub struct Page(FrameTracker);

impl DevicePage for Page {
    fn as_mut_slice(&mut self) -> &mut [u8] {
        self.0.as_mut_slice_with(0)
    }

    fn as_slice(&self) -> &[u8] {
        self.0.as_slice_with(0)
    }

    fn paddr(&self) -> PhysAddr {
        self.0.start_phy_addr().as_usize()
    }

    fn vaddr(&self) -> VirtAddr {
        self.0.start_virt_addr().as_usize()
    }
}

impl<const SIZE: usize> QueuePage<SIZE> for Page {
    fn queue_ref_mut(&mut self, layout: &QueueLayout) -> QueueMutRef<SIZE> {
        let desc_table_offset = layout.descriptor_table_offset;
        let table = self.0.as_mut_slice_with(desc_table_offset);
        let avail_ring_offset = layout.avail_ring_offset;
        let avail_ring = self.0.as_mut_with(avail_ring_offset);

        let used_ring_offset = layout.used_ring_offset;
        let used_ring = self.0.as_mut_with(used_ring_offset);
        QueueMutRef {
            descriptor_table: table,
            avail_ring,
            used_ring,
        }
    }
}

pub struct HalImpl;
impl<const SIZE: usize> Hal<SIZE> for HalImpl {
    fn dma_alloc(pages: usize) -> Box<dyn QueuePage<SIZE>> {
        let frame = FrameTracker::new(pages);
        frame.clear();
        Box::new(Page(frame))
    }

    fn dma_alloc_buf(pages: usize) -> Box<dyn DevicePage> {
        let frame = FrameTracker::new(pages);
        frame.clear();
        Box::new(Page(frame))
    }

    fn to_paddr(va: usize) -> usize {
        basic::vaddr_to_paddr_in_kernel(va).unwrap_or(va)
    }
}

pub fn to_alien_err(e: VirtIoError) -> AlienError {
    log::error!("{:?}", e);
    AlienError::DOMAINCRASH
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

pub fn transport_from_init_info(
    init_info: &VirtioInitInfo,
    dev_type: DeviceType,
    dev_name: &str,
) -> VirtioTransport {
    match init_info {
        VirtioInitInfo::Mmio { range, .. } => {
            basic::println!("{}_mmio: {:#x}-{:#x}", dev_name, range.start, range.end);
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
            if let (Some(common), Some(notify), Some(notify_mul), Some(isr), Some(device_cfg)) = (
                modern_common.clone(),
                modern_notify.clone(),
                *modern_notify_off_multiplier,
                modern_isr.clone(),
                modern_device.clone(),
            ) {
                basic::println!(
                    "{}_pci(modern): {:04x}:{:02x}:{:02x}.{}",
                    dev_name,
                    segment,
                    bus,
                    device,
                    function
                );
                VirtioTransport::PciModern(
                    ModernPciTransport::new(
                        Box::new(SafeIORW(SafeIORegion::from(common))),
                        Box::new(SafeIORW(SafeIORegion::from(notify))),
                        Box::new(SafeIORW(SafeIORegion::from(isr))),
                        Box::new(SafeIORW(SafeIORegion::from(device_cfg))),
                        notify_mul,
                        dev_type,
                    )
                    .unwrap(),
                )
            } else if let Some(io_range) = legacy_io.clone() {
                basic::println!(
                    "{}_pci(legacy): {:04x}:{:02x}:{:02x}.{}, io={:#x}-{:#x}",
                    dev_name,
                    segment,
                    bus,
                    device,
                    function,
                    io_range.start,
                    io_range.end
                );
                let io_region = SafeIORW(SafeIORegion::from(io_range));
                VirtioTransport::PciLegacy(
                    LegacyPciTransport::new(Box::new(io_region), dev_type).unwrap(),
                )
            } else {
                panic!("{} pci has no usable transport info", dev_name);
            }
        }
    }
}
