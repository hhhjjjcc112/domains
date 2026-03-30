#![no_std]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "riscv64"))]
compile_error!("vf2_sd domain 仅支持 riscv64");

#[cfg(feature = "fs_test")]
mod fs_test;
mod ops;

extern crate alloc;
use alloc::boxed::Box;
use core::fmt::Debug;

use basic::{io::SafeIORegion, println, println_color, sync::Mutex, AlienResult};
use interface::{define_unwind_for_BlkDeviceDomain, Basic, BlkDeviceDomain, DeviceBase, VirtioInitInfo};
use shared_heap::{DBox, DVec};
use visionfive2_sd::Vf2SdDriver;

use crate::ops::{SdIoImpl, SleepOpsImpl};

pub struct Vf2SDCardDomain {
    sd: Mutex<Vf2SdDriver<SdIoImpl, SleepOpsImpl>>,
}

impl Debug for Vf2SDCardDomain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Vf2SDCardDomain")
    }
}

impl Vf2SDCardDomain {
    pub fn empty() -> Self {
        let io = SafeIORegion::from(0..0);
        Self {
            sd: Mutex::new(Vf2SdDriver::new(SdIoImpl::new(io))),
        }
    }
}

impl DeviceBase for Vf2SDCardDomain {
    fn handle_irq(&self) -> AlienResult<()> {
        unimplemented!()
    }
}

impl Basic for Vf2SDCardDomain {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl BlkDeviceDomain for Vf2SDCardDomain {
    fn init(&self, device_info: &VirtioInitInfo) -> AlienResult<()> {
        let region = device_info
            .mmio_range()
            .expect("vf2_sd init requires mmio range");
        println!("sd card: {:#x}-{:#x}", region.start, region.end);
        let io_region = SafeIORegion::from(region.clone());
        // preprint::init_print(&PrePrint);
        let mut sd = Vf2SdDriver::new(SdIoImpl::new(io_region));
        sd.init();
        println_color!(32, "sd card init success");
        let mut buf = [0; 512];
        sd.read_block(0, &mut buf);
        println!("buf: {:x?}", &buf[..16]);
        #[cfg(feature = "fs_test")]
        fs_test::init_fatfs(sd);
        #[cfg(not(feature = "fs_test"))]
        {
            *self.sd.lock() = sd;
        }
        Ok(())
    }

    fn read_block(&self, block: u32, mut data: DVec<u8>) -> AlienResult<DVec<u8>> {
        self.sd
            .lock()
            .read_block(block as usize, data.as_mut_slice());
        Ok(data)
    }

    fn write_block(&self, block: u32, data: &DVec<u8>) -> AlienResult<usize> {
        self.sd.lock().write_block(block as usize, data.as_slice());
        Ok(data.len())
    }

    fn get_capacity(&self) -> AlienResult<u64> {
        Ok(32 * 1024 * 1024 * 1024 / 512)
    }

    fn flush(&self) -> AlienResult<()> {
        Ok(())
    }
}

define_unwind_for_BlkDeviceDomain!(Vf2SDCardDomain);

pub fn main() -> Box<dyn BlkDeviceDomain> {
    Box::new(UnwindWrap::new(Vf2SDCardDomain::empty()))
}
