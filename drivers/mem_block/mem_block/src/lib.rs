#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
use alloc::boxed::Box;
use core::cmp::min;

use basic::{io::SafeIORegion, println, sync::Mutex, AlienError, AlienResult};
use interface::{define_unwind_for_BlkDeviceDomain, Basic, BlkDeviceDomain, DeviceBase, VirtioInitInfo};
use shared_heap::DVec;

#[derive(Debug)]
pub struct MemoryImg {
    data: Mutex<SafeIORegion>,
}

impl Default for MemoryImg {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryImg {
    pub fn new() -> Self {
        MemoryImg {
            data: Mutex::new(SafeIORegion::from(0..0)),
        }
    }

    pub fn read_blocks(&self, block: u64, data: &mut [u8]) -> AlienResult<usize> {
        if data.len() % 512 != 0 {
            return Err(AlienError::EINVAL);
        }
        let start = block as usize * 512;
        let end = start + data.len();
        let datalock = self.data.lock();
        let io_region = datalock.as_bytes();
        let copy_start = min(io_region.len(), start);
        let copy_end = min(io_region.len(), end);
        data[..copy_end - copy_start].copy_from_slice(&io_region[copy_start..copy_end]);
        Ok(copy_end - copy_start)
    }

    pub fn write_blocks(&self, block: u64, data: &[u8]) -> AlienResult<usize> {
        if data.len() % 512 != 0 {
            return Err(AlienError::EINVAL);
        }
        let start = block as usize * 512;
        let end = start + data.len();
        // let io_region = self.data.lock().as_mut_bytes();

        let mut data_lock = self.data.lock();
        let io_region = data_lock.as_mut_bytes();

        let copy_start = min(io_region.len(), start);
        let copy_end = min(io_region.len(), end);
        io_region[copy_start..copy_end].copy_from_slice(&data[..copy_end - copy_start]);
        Ok(copy_end - copy_start)
    }
}

impl DeviceBase for MemoryImg {
    fn handle_irq(&self) -> AlienResult<()> {
        todo!()
    }
}

impl Basic for MemoryImg {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl BlkDeviceDomain for MemoryImg {
    fn init(&self, device_info: &VirtioInitInfo) -> AlienResult<()> {
        let region = device_info
            .mmio_range()
            .expect("mem_block init requires mmio range");
        println!("mem block: {:#x}-{:#x}", region.start, region.end);
        let io_region = SafeIORegion::from(region.clone());
        *self.data.lock() = io_region;
        Ok(())
    }
    fn read_block(&self, block: u32, mut data: DVec<u8>) -> AlienResult<DVec<u8>> {
        self.read_blocks(block as _, data.as_mut_slice())?;
        Ok(data)
    }
    fn write_block(&self, block: u32, data: &DVec<u8>) -> AlienResult<usize> {
        self.write_blocks(block as _, data.as_slice())
    }
    fn get_capacity(&self) -> AlienResult<u64> {
        Ok(self.data.lock().size() as u64 / 512)
    }
    fn flush(&self) -> AlienResult<()> {
        Ok(())
    }
}

define_unwind_for_BlkDeviceDomain!(MemoryImg);

pub fn main() -> Box<dyn BlkDeviceDomain> {
    Box::new(UnwindWrap::new(MemoryImg::new()))
}
