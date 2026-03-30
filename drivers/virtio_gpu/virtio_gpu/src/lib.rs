#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::boxed::Box;
use core::{fmt::Debug, ops::Range};

use basic::{
    io::SafeIORegion,
    println,
    sync::{Mutex, Once, OnceGet},
    AlienResult,
};
use interface::{define_unwind_for_GpuDomain, Basic, DeviceBase, GpuDomain, VirtioInitInfo};
use shared_heap::DVec;
use virtio_drivers::error::VirtIoResult;
use virtio_drivers::hal::VirtIoDeviceIo;
use virtio_drivers::transport::{DeviceStatus, DeviceType, Transport};
use virtio_drivers::transport::mmio::MmioTransport;
use virtio_drivers::transport::pci::{LegacyPciTransport, ModernPciTransport};
use virtio_drivers::device::gpu::VirtIOGpu;
use virtio_mmio_common::{HalImpl, SafeIORW};

pub struct GPUDomain {
    buffer_range: Once<Range<usize>>,
    gpu: Once<Mutex<VirtIOGpu<HalImpl, VirtioTransport>>>,
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

impl Debug for GPUDomain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("GPUDomain")
    }
}

impl Default for GPUDomain {
    fn default() -> Self {
        Self::new()
    }
}

impl GPUDomain {
    pub fn new() -> Self {
        Self {
            buffer_range: Once::new(),
            gpu: Once::new(),
        }
    }
}

impl Basic for GPUDomain {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl DeviceBase for GPUDomain {
    fn handle_irq(&self) -> AlienResult<()> {
        unimplemented!()
    }
}

impl GpuDomain for GPUDomain {
    fn init(&self, init_info: &VirtioInitInfo) -> AlienResult<()> {
        let transport = match init_info {
            VirtioInitInfo::Mmio { range, .. } => {
                println!("virtio_gpu_mmio: {:#x}-{:#x}", range.start, range.end);
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
                        "virtio_gpu_pci(modern): {:04x}:{:02x}:{:02x}.{}",
                        segment, bus, device, function
                    );
                    VirtioTransport::PciModern(
                        ModernPciTransport::new(
                            Box::new(SafeIORW(SafeIORegion::from(common))),
                            Box::new(SafeIORW(SafeIORegion::from(notify))),
                            Box::new(SafeIORW(SafeIORegion::from(isr))),
                            Box::new(SafeIORW(SafeIORegion::from(device_cfg))),
                            notify_mul,
                            DeviceType::GPU,
                        )
                        .unwrap(),
                    )
                } else if let Some(io_range) = legacy_io.clone() {
                    println!(
                        "virtio_gpu_pci(legacy): {:04x}:{:02x}:{:02x}.{}, io={:#x}-{:#x}",
                        segment, bus, device, function, io_range.start, io_range.end
                    );
                    let io_region = SafeIORW(SafeIORegion::from(io_range));
                    VirtioTransport::PciLegacy(
                        LegacyPciTransport::new(Box::new(io_region), DeviceType::GPU).unwrap(),
                    )
                } else {
                    panic!("virtio-gpu pci has no usable transport info");
                }
            }
        };
        let mut gpu =
            VirtIOGpu::<HalImpl, VirtioTransport>::new(transport).expect("failed to create gpu driver");

        let (width, height) = gpu.resolution().expect("failed to get resolution");
        let width = width as usize;
        let height = height as usize;
        println!("GPU resolution is {}x{}", width, height);
        let fb = gpu.setup_framebuffer().expect("failed to get fb");
        let buffer_range = fb.as_ptr() as usize..(fb.as_ptr() as usize + fb.len());
        gpu.move_cursor(50, 50).unwrap();
        gpu.flush().unwrap();
        self.buffer_range.call_once(|| buffer_range);
        self.gpu.call_once(|| Mutex::new(gpu));
        Ok(())
    }

    fn flush(&self) -> AlienResult<()> {
        self.gpu.get_must().lock().flush().unwrap();
        Ok(())
    }

    fn fill(&self, _offset: u32, _buf: &DVec<u8>) -> AlienResult<usize> {
        todo!()
    }

    fn buffer_range(&self) -> AlienResult<Range<usize>> {
        let x = self.buffer_range.get_must().clone();
        Ok(x)
    }
}

define_unwind_for_GpuDomain!(GPUDomain);

pub fn main() -> Box<dyn GpuDomain> {
    Box::new(UnwindWrap::new(GPUDomain::new()))
}
