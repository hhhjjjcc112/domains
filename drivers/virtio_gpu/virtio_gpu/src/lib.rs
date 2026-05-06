#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::boxed::Box;
use core::{fmt::Debug, ops::Range};

use basic::{
    println,
    sync::{Mutex, Once, OnceGet},
    AlienResult,
};
use interface::{define_unwind_for_GpuDomain, Basic, DeviceBase, GpuDomain, VirtioInitInfo};
use shared_heap::DVec;
use virtio_drivers::device::gpu::VirtIOGpu;
use virtio_drivers::transport::DeviceType;
use virtio_common::{transport_from_init_info, HalImpl, VirtioTransport};

pub struct GPUDomain {
    buffer_range: Once<Range<usize>>,
    gpu: Once<Mutex<VirtIOGpu<HalImpl, VirtioTransport>>>,
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
        self.gpu.get_must().lock().ack_interrupt().unwrap();
        Ok(())
    }
}

impl GpuDomain for GPUDomain {
    fn init(&self, init_info: &VirtioInitInfo) -> AlienResult<()> {
        let transport = transport_from_init_info(init_info, DeviceType::GPU, "virtio_gpu");
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
