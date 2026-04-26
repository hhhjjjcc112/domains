use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::mem::offset_of;

use basic::{
    AlienError, AlienResult,
    config::FRAME_SIZE,
    constants::io::{MMapFlags, ProtFlags},
    sync::Mutex,
    time,
    vaddr_to_paddr_in_kernel,
    vm::frame::FrameTracker,
};
use page_table::MappingFlags;
use ptable::{PhysPage, VmArea, VmAreaType};
use vdso_api::{MemIf, MappingFlags as VdsoMappingFlags, UserMemIf, VvarData};

use crate::{
    elf::FrameTrackerWrapper,
    processor::current_task,
    resource::MMapRegion,
};

static VDSO_SHARED_FRAME: Mutex<Option<Arc<FrameTracker>>> = Mutex::new(None);
static VDSO_PRIVATE_FRAMES: Mutex<Vec<Arc<FrameTracker>>> = Mutex::new(Vec::new());
static VDSO_LOADED: Mutex<bool> = Mutex::new(false);

fn page_align(size: usize) -> usize {
	(size + FRAME_SIZE - 1) & !(FRAME_SIZE - 1)
}

struct KernelVdsoMem;

#[crate_interface::impl_interface]
impl MemIf for KernelVdsoMem {
    fn alloc(size: usize) -> *mut u8 {
        assert_eq!(size % FRAME_SIZE, 0);
        let mut shared_frame = VDSO_SHARED_FRAME.lock();
        if shared_frame.is_none() {
            let frame = Arc::new(FrameTracker::new(size / FRAME_SIZE));
            frame.clear();
            let ptr = frame.start_virt_addr().as_usize() as *mut u8;
            *shared_frame = Some(Arc::clone(&frame));
            return ptr;
        }

        drop(shared_frame);

        let frame = Arc::new(FrameTracker::new(size / FRAME_SIZE));
        frame.clear();
        let ptr = frame.start_virt_addr().as_usize() as *mut u8;
        VDSO_PRIVATE_FRAMES.lock().push(frame);
        ptr
    }

    fn protect(addr: *mut u8, len: usize, flags: VdsoMappingFlags) {
        // 当前阶段不再把权限变化下沉到 kernel 页表，直接保留空实现。
        let _ = (addr, len, flags);
    }
}

struct UserVdsoMem;

#[crate_interface::impl_interface]
impl UserMemIf for UserVdsoMem {
    fn ualloc(_vspace: usize, size: usize) -> *mut u8 {
        let task = current_task().unwrap();

        assert_eq!(size % FRAME_SIZE, 0);
        let mut mmap = task.mmap.lock();
        let v_range = mmap.alloc(size);
        // 标记这段地址先按匿名读写区域登记，后面 map 再落成最终权限。
        let region = MMapRegion::new(
            v_range.start,
            size,
            v_range.end - v_range.start,
            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
            MMapFlags::MAP_ANONYMOUS,
            None,
            0,
        );
        mmap.add_region(region);

        v_range.start as *mut u8
    }

    fn map(_vspace: usize, user_addr: *mut u8, kaddr: *mut u8, len: usize, flags: VdsoMappingFlags) {
        let task = current_task().unwrap();

        let len = page_align(len);
        let page_count = len / FRAME_SIZE;
        let mut phy_frames: Vec<Box<dyn PhysPage>> = Vec::with_capacity(page_count);

        for page_index in 0..page_count {
            let page_kaddr = kaddr.wrapping_add(page_index * FRAME_SIZE);
            let page_paddr = vaddr_to_paddr_in_kernel(page_kaddr as usize)
                .expect("vDSO kernel backing is not mapped into kernel direct map");
            let frame = FrameTracker::from_phy_range(page_paddr..page_paddr + FRAME_SIZE);
            phy_frames.push(Box::new(FrameTrackerWrapper(frame)));
        }

        // 按用户权限更新 mmap 记录，保持 user-side 语义和页表一致。
        let mut mmap = task.mmap.lock();
        let user_addr = user_addr as usize;
        if let Some(region) = mmap.get_region_mut(user_addr) {
            // vDSO 区域最终权限由 loader 传入，mmap 记录同步成同样的权限。
            region.set_prot(ProtFlags::from_bits_truncate(flags.bits() as u32));
        }

        // 最终把这段区域映射进当前进程的地址空间。
        let map_flags = MappingFlags::from_bits_truncate(flags.bits());
        let area = VmArea::new(user_addr..(user_addr + len), map_flags, phy_frames);
        task.address_space
            .lock()
            .map(VmAreaType::VmArea(area))
            .unwrap();
    }
}

fn ensure_loaded() {
    let mut loaded = VDSO_LOADED.lock();
    if !*loaded {
        vdso_api::load_and_init();
        refresh_vvar_snapshot();
        *loaded = true;
    }
}

fn refresh_vvar_snapshot() {
    let shared_frame = VDSO_SHARED_FRAME.lock();
    let Some(shared_frame) = shared_frame.as_ref() else {
        return;
    };

    let seq_off = offset_of!(VvarData, seq);
    let realtime_off = offset_of!(VvarData, realtime_ns);
    let monotonic_off = offset_of!(VvarData, monotonic_ns);
    let realtime = time::wall_time_nanos() as usize;
    let monotonic = time::monotonic_time_nanos() as usize;

    let mut seq = shared_frame.read_value_atomic(seq_off);
    if seq & 1 != 0 {
        seq = seq.wrapping_add(1);
    }

    let writing_seq = seq.wrapping_add(1);
    let stable_seq = writing_seq.wrapping_add(1);

    shared_frame.write_value_atomic(seq_off, writing_seq);
    shared_frame.write_value_atomic(realtime_off, realtime);
    shared_frame.write_value_atomic(monotonic_off, monotonic);
    shared_frame.write_value_atomic(seq_off, stable_seq);
}

pub(crate) fn update_time_snapshot() {
    refresh_vvar_snapshot();
}

pub(crate) fn load_vdso() -> AlienResult<usize> {
    ensure_loaded();

    let regions = vdso_api::map_and_init(0);
    let user_vdso_base = regions
        .get(1)
        .map(|region| region.0 as usize)
        .ok_or(AlienError::EINVAL)?;

    Ok(user_vdso_base)
}