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
use interface::{define_unwind_for_GpuDomain, Basic, DeviceBase, GpuDomain};
use shared_heap::DVec;
#[cfg(target_arch = "riscv64")]
use virtio_drivers::transport::mmio::MmioTransport;
#[cfg(target_arch = "x86_64")]
use virtio_drivers::transport::{
    pci::{bus::{Cam, Command, PciRoot}, virtio_device_type, PciTransport},
    DeviceType,
};
use virtio_drivers::device::gpu::VirtIOGpu;
use virtio_mmio_common::{HalImpl, SafeIORW};

pub struct GPUDomain {
    buffer_range: Once<Range<usize>>,
    #[cfg(target_arch = "riscv64")]
    gpu: Once<Mutex<VirtIOGpu<HalImpl, MmioTransport>>>,
    #[cfg(target_arch = "x86_64")]
    gpu: Once<Mutex<VirtIOGpu<HalImpl, PciTransport>>>,
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
    fn init(&self, address_range: &Range<usize>) -> AlienResult<()> {
        #[cfg(target_arch = "riscv64")]
        {
            let virtio_gpu_addr = address_range.start;
            println!("virtio_gpu_addr: {:#x?}", virtio_gpu_addr);
            let io_region = SafeIORW(SafeIORegion::from(address_range.clone()));
            let transport = MmioTransport::new(Box::new(io_region)).unwrap();
            let mut gpu = VirtIOGpu::<HalImpl, MmioTransport>::new(transport)
                .expect("failed to create gpu driver");

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
            return Ok(());
        }

        #[cfg(target_arch = "x86_64")]
        {
            let virtio_gpu_addr = address_range.start;
            println!("virtio_gpu_addr: {:#x?}", virtio_gpu_addr);
            let io_region = SafeIORW(SafeIORegion::from(address_range.clone()));
            let mut root = PciRoot::new(Box::new(io_region), Cam::Ecam);

            let mut found = None;
            'outer: for bus in 0u8..=u8::MAX {
                for (device_function, info) in root.enumerate_bus(bus) {
                    if virtio_device_type(&info) == Some(DeviceType::GPU) {
                        found = Some((device_function, info));
                        break 'outer;
                    }
                }
            }

            let (device_function, info) = found.expect("virtio-pci gpu not found in ECAM");
            println!("virtio-pci gpu bdf={} ({})", device_function, info);

            let (_status, mut command) = root.get_status_command(device_function);
            command.insert(Command::BUS_MASTER | Command::MEMORY_SPACE | Command::IO_SPACE);
            root.set_command(device_function, command);

            let transport = PciTransport::new::<HalImpl>(&mut root, device_function)
                .expect("failed to create virtio-pci transport");
            let mut gpu = VirtIOGpu::<HalImpl, PciTransport>::new(transport)
                .expect("failed to create virtio-pci gpu");

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
            return Ok(());
        }

        #[allow(unreachable_code)]
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
