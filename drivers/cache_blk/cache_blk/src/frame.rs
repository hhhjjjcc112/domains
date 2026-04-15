#![allow(dead_code)]

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{
    cmp::min,
    fmt::Debug,
    num::NonZeroUsize,
    ops::{Deref, DerefMut, Index, IndexMut},
};

use basic::{
    config::FRAME_SIZE,
    println_color,
    sync::{Mutex, Once, OnceGet},
    vm::frame::FrameTracker,
    AlienResult,
};
use interface::{
    define_unwind_for_CacheBlkDeviceDomain, Basic, CacheBlkDeviceDomain, DeviceBase, DomainType,
    ShadowBlockDomain,
};
use log::info;
use lru::LruCache;
use shared_heap::DVec;

struct PageCache(FrameTracker);

impl Index<usize> for PageCache {
    type Output = [u8];

    fn index(&self, index: usize) -> &Self::Output {
        &self.0.deref()[index * 512..(index + 1) * 512]
    }
}

impl IndexMut<usize> for PageCache {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0.deref_mut()[index * 512..(index + 1) * 512]
    }
}

pub struct GenericBlockDevice {
    cache: Mutex<LruCache<usize, PageCache>>,
    dirty: Mutex<Vec<usize>>,
    blk: Once<Arc<dyn ShadowBlockDomain>>,
}

impl Debug for GenericBlockDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GenericBlockDevice").finish()
    }
}

impl GenericBlockDevice {
    pub fn new(max_cache_frames: usize) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(max_cache_frames).unwrap())),
            dirty: Mutex::new(Vec::new()),
            blk: Once::new(),
        }
    }

    fn check(&self, page_id: usize) {
        let mut cache_lock = self.cache.lock();
        // static MASK: AtomicBool = AtomicBool::new(true);
        if !cache_lock.contains(&page_id) {
            // let now = read_time_us();
            let device = self.blk.get_must();
            // todo!(change interface)
            let start_block = page_id * FRAME_SIZE / 512;
            let end_block = start_block + FRAME_SIZE / 512;
            let mut frame = FrameTracker::new(1);
            for i in start_block..end_block {
                let target_buf = &mut frame[(i - start_block) * 512..(i - start_block + 1) * 512];
                let cache_slice = DVec::from_other_rvec_slice(target_buf);
                let _cache_slice = device.read_block(i as u32, cache_slice).unwrap();
            }
            let cache = PageCache(frame);
            let old_cache = cache_lock.push(page_id, cache);
            if let Some((id, old_cache)) = old_cache {
                let start_block = id * FRAME_SIZE / 512;
                let end_block = start_block + FRAME_SIZE / 512;
                for i in start_block..end_block {
                    let target_buf =
                        &old_cache.0[(i - start_block) * 512..(i - start_block + 1) * 512];
                    let tmp_buf = DVec::from_other_rvec_slice(target_buf);
                    device.write_block(i as u32, &tmp_buf).unwrap();
                    self.dirty.lock().retain(|&x| x != id);
                }
            }
            // let end = read_time_us();
            // if MASK.load(core::sync::atomic::Ordering::Relaxed) {
            //     println_color!(31, "read block: {}us", end - now);
            //     MASK.store(false, core::sync::atomic::Ordering::Relaxed);
            // }
        }
    }
}

impl Basic for GenericBlockDevice {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl DeviceBase for GenericBlockDevice {
    fn handle_irq(&self) -> AlienResult<()> {
        self.blk.get_must().handle_irq()
    }
}

impl CacheBlkDeviceDomain for GenericBlockDevice {
    fn init(&self, blk_domain_name: &str) -> AlienResult<()> {
        let blk = basic::get_domain(blk_domain_name).unwrap();
        match blk {
            DomainType::ShadowBlockDomain(blk) => {
                info!(
                    "max_cache_frames: {}, blk size: {}MB",
                    MAX_BLOCK_CACHE_FRAMES,
                    blk.get_capacity().unwrap() / (1024 * 1024)
                );
                self.blk.call_once(|| blk);
                Ok(())
            }
            _ => {
                panic!("blk domain not found");
            }
        }
    }

    fn read(&self, offset: u64, mut buf: DVec<u8>) -> AlienResult<DVec<u8>> {
        let mut page_id = offset as usize / FRAME_SIZE;
        let mut offset = offset as usize % FRAME_SIZE;
        let len = buf.len();
        let mut count = 0;
        while count < len {
            self.check(page_id);
            let mut cache_lock = self.cache.lock();
            let cache = cache_lock.get(&page_id).unwrap();
            let copy_len = min(FRAME_SIZE - offset, len - count);
            // cache.copy_to(offset, &mut buf.as_mut_slice()[count..count + copy_len]);

            buf.as_mut_slice()[count..count + copy_len]
                .copy_from_slice(&cache.0[offset..offset + copy_len]);

            count += copy_len;
            offset = 0;
            page_id += 1;
        }
        Ok(buf)
    }

    fn write(&self, offset: u64, buf: &DVec<u8>) -> AlienResult<usize> {
        let mut page_id = offset as usize / FRAME_SIZE;
        let mut offset = offset as usize % FRAME_SIZE;
        let len = buf.len();
        let mut count = 0;
        while count < len {
            self.check(page_id);
            let mut cache_lock = self.cache.lock();
            let cache = cache_lock.get_mut(&page_id).unwrap();
            let copy_len = min(FRAME_SIZE - offset, len - count);
            // cache.copy_from(offset, &buf.as_slice()[count..count + copy_len]);
            cache.0[offset..offset + copy_len]
                .copy_from_slice(&buf.as_slice()[count..count + copy_len]);
            count += copy_len;
            offset = (offset + copy_len) % FRAME_SIZE;
            page_id += 1;
        }
        Ok(buf.len())
    }

    fn get_capacity(&self) -> AlienResult<u64> {
        self.blk.get_must().get_capacity()
    }

    fn flush(&self) -> AlienResult<()> {
        Ok(())
    }
}

pub const MAX_BLOCK_CACHE_FRAMES: usize = 1024 * 4 * 4;

define_unwind_for_CacheBlkDeviceDomain!(GenericBlockDevice);

pub fn main() -> Box<dyn CacheBlkDeviceDomain> {
    println_color!(31, "GenericBlockDevice with frame");
    Box::new(UnwindWrap::new(GenericBlockDevice::new(
        MAX_BLOCK_CACHE_FRAMES,
    )))
}
