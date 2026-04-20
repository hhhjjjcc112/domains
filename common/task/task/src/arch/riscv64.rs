use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::mem::offset_of;

use basic::{
    AlienError, AlienResult,
    config::FRAME_SIZE,
    constants::io::{MMapFlags, ProtFlags},
    sync::Mutex,
    time,
    task::{TaskContext, TrapFrame},
    vm::frame::FrameTracker,
};
use memory_addr::VirtAddr;
use page_table::{MappingFlags, PagingError};
use ptable::{PhysPage, VmArea, VmAreaType, VmIo, VmSpace};
use vdso_api::{KernelMemIf, MappingFlags as VdsoMappingFlags, UserMemIf, VdsoLayout, VvarData};

use super::UserArchState;
use crate::{
    elf::{FrameTrackerWrapper, VmmPageAllocator},
    processor::current_task,
    resource::MMapRegion,
};

static VDSO_SHARED_FRAME: Mutex<Option<Arc<FrameTracker>>> = Mutex::new(None);
static VDSO_LAYOUT: Mutex<Option<VdsoLayout>> = Mutex::new(None);

fn page_align(size: usize) -> usize {
	(size + FRAME_SIZE - 1) & !(FRAME_SIZE - 1)
}

pub(super) fn arch_initial_user_state(tls: usize) -> UserArchState {
    UserArchState::new(tls, 0)
}

pub(super) fn arch_current_user_state() -> AlienResult<UserArchState> {
    Ok(UserArchState::default())
}

pub(super) fn arch_clone_user_state(
    parent_state: UserArchState,
    set_tls: bool,
    tls: usize,
) -> UserArchState {
    if set_tls {
        UserArchState::new(tls, 0)
    } else {
        parent_state
    }
}

pub(super) fn arch_apply_user_state(_context: &mut TaskContext, _state: UserArchState) {}

pub(super) fn arch_set_current_user_state(_state: UserArchState) -> AlienResult<()> {
    Ok(())
}

pub(super) fn arch_apply_trap_tls(trap_frame: &mut TrapFrame, tls: usize) {
    trap_frame.update_tls(tls.into());
}

pub(super) fn arch_validate_interp_path(path: &str) {
    assert!(path.starts_with("/lib/ld-musl-riscv64"));
}

pub(super) fn arch_relocate_dyn(
    elf: &xmas_elf::ElfFile<'_>,
    bias: usize,
) -> AlienResult<Vec<(usize, usize)>> {
    let mut res = Vec::new();
    let data = elf
        .find_section_by_name(".rela.dyn")
        .unwrap()
        .get_data(elf)
        .unwrap();
    let entries = match data {
        xmas_elf::sections::SectionData::Rela64(entries) => entries,
        _ => return Err(AlienError::EINVAL),
    };
    let dynsym = match elf
        .find_section_by_name(".dynsym")
        .unwrap()
        .get_data(elf)
        .unwrap()
    {
        xmas_elf::sections::SectionData::DynSymbolTable64(dsym) => dsym,
        _ => return Err(AlienError::EINVAL),
    };

    const REL_SYM_ABS: u32 = 2;
    const REL_RELATIVE: u32 = 3;

    for entry in entries.iter() {
        match entry.get_type() {
            REL_SYM_ABS => {
                let dynsym = &dynsym[entry.get_symbol_table_index() as usize];
                let symval = if dynsym.shndx() == 0 {
                    let name = dynsym
                        .get_name(elf)
                        .map_err(|_| AlienError::EINVAL)?;
                    panic!("need to find symbol: {:?}", name);
                } else {
                    bias + dynsym.value() as usize
                };
                let value = symval + entry.get_addend() as usize;
                let addr = bias + entry.get_offset() as usize;
                res.push((addr, value));
            }
            REL_RELATIVE => {
                let value = bias + entry.get_addend() as usize;
                let addr = bias + entry.get_offset() as usize;
                res.push((addr, value));
            }
            t => unimplemented!("unknown type: {}", t),
        }
    }
    Ok(res)
}

pub(super) fn arch_relocate_plt(
    elf: &xmas_elf::ElfFile<'_>,
    bias: usize,
) -> AlienResult<Vec<(usize, usize)>> {
    let mut res = Vec::new();
    let data = elf
        .find_section_by_name(".rela.plt")
        .ok_or(AlienError::EINVAL)?
        .get_data(elf)
        .map_err(|_| AlienError::EINVAL)?;
    let entries = match data {
        xmas_elf::sections::SectionData::Rela64(entries) => entries,
        _ => return Err(AlienError::EINVAL),
    };
    let dynsym = match elf
        .find_section_by_name(".dynsym")
        .unwrap()
        .get_data(elf)
        .unwrap()
    {
        xmas_elf::sections::SectionData::DynSymbolTable64(dsym) => dsym,
        _ => return Err(AlienError::EINVAL),
    };

    const REL_PLT: u32 = 5;

    for entry in entries.iter() {
        match entry.get_type() {
            REL_PLT => {
                let dynsym = &dynsym[entry.get_symbol_table_index() as usize];
                let symval = if dynsym.shndx() == 0 {
                    let name = dynsym
                        .get_name(elf)
                        .map_err(|_| AlienError::EINVAL)?;
                    panic!("symbol not found: {:?}", name);
                } else {
                    dynsym.value() as usize
                };
                let value = bias + symval;
                let addr = bias + entry.get_offset() as usize;
                res.push((addr, value));
            }
            t => panic!("[kernel] unknown entry, type = {}", t),
        }
    }
    Ok(res)
}

pub(super) fn arch_map_extra_user_regions(
    _address_space: &mut VmSpace<VmmPageAllocator>,
) -> AlienResult<()> {
    Ok(())
}

struct KernelVdsoMem;

#[crate_interface::impl_interface]
impl KernelMemIf for KernelVdsoMem {
    fn alloc(size: usize) -> *mut u8 {
        // 先按页分配一份共享 backing，后续所有进程都复用同一组物理页。
        assert_eq!(size % FRAME_SIZE, 0);
        let page_count = size / FRAME_SIZE;
        let frame = {
            let mut shared_frame = VDSO_SHARED_FRAME.lock();
            if let Some(frame) = shared_frame.as_ref() {
                Arc::clone(frame)
            } else {
                let frame = Arc::new(FrameTracker::new(page_count));
                frame.clear();
                *shared_frame = Some(Arc::clone(&frame));
                frame
            }
        };
        // 返回共享 backing 的内核可写起始地址，供 loader 直接填充内容。
        frame.start_virt_addr().as_usize() as *mut u8
    }

    fn protect(addr: *mut u8, len: usize, flags: VdsoMappingFlags) {
        // 当前阶段不再把权限变化下沉到 kernel 页表，直接保留空实现。
        let _ = (addr, len, flags);
    }
}

struct UserVdsoMem;

#[crate_interface::impl_interface]
impl UserMemIf for UserVdsoMem {
    fn alloc(size: usize) -> *mut u8 {
        // 为当前进程预留一段连续虚拟地址，后续按段落把 vDSO 内容映射进去。
        let task = current_task().unwrap();
        assert_eq!(size % FRAME_SIZE, 0);
        let mut mmap = task.mmap.lock();
        let v_range = mmap.alloc(size);
        // 标记这段地址先按匿名读写区域登记，后面 protect 再落成最终权限。
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

    fn protect(phys_addr: *mut u8, user_addr: *mut u8, len: usize, flags: VdsoMappingFlags, shared: bool) {
        // 先按页对齐，确保共享页和私有页的映射边界都落在完整页上。
        let len = page_align(len);
        let page_count = len / FRAME_SIZE;

        // 共享 backing 仍然来自同一份全局 frame，私有页则从这里拷贝初值。
        let shared_frame = {
            let shared_frame = VDSO_SHARED_FRAME.lock();
            shared_frame
                .as_ref()
                .cloned()
                .unwrap_or_else(|| panic!("vDSO backing is not initialized"))
        };

        // 共享段直接引用共享 backing 的同一页；私有段按页复制出独立副本。
        let offset = (phys_addr as usize).saturating_sub(shared_frame.start_virt_addr().as_usize());
        let user_addr = user_addr as usize;
        let shared_region = shared;
        let mut phy_frames: Vec<Box<dyn PhysPage>> = Vec::with_capacity(page_count);
        if shared_region {
            // 共享段不复制内容，只把每一页包装成页表可消费的只读/共享页。
            let page_index_base = offset / FRAME_SIZE;
            for page_index in 0..page_count {
                let page_start = shared_frame.start_phy_addr().as_usize() + (page_index_base + page_index) * FRAME_SIZE;
                phy_frames.push(Box::new(FrameTrackerWrapper(FrameTracker::from_phy_range(
                    page_start..page_start + FRAME_SIZE,
                ))));
            }
        } else {
            // 私有段按页拷贝初始内容，保证每个进程拿到自己的 .data / .bss。
            for page_index in 0..page_count {
                let frame = FrameTracker::new(1);
                let src = shared_frame
                    .as_slice_with::<u8>(offset + page_index * FRAME_SIZE)
                    .get(..FRAME_SIZE)
                    .unwrap();
                frame.as_mut_slice_with::<u8>(0).copy_from_slice(src);
                phy_frames.push(Box::new(FrameTrackerWrapper(frame)));
            }
        }

        // 按用户权限更新 mmap 记录，保持 user-side 语义和页表一致。
        let task = current_task().unwrap();
        let mut mmap = task.mmap.lock();
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

pub(super) fn arch_load_vdso() -> AlienResult<usize> {
    let layout = {
        let mut cached_layout = VDSO_LAYOUT.lock();
        if let Some(layout) = cached_layout.as_ref() {
            layout.clone()
        } else {
            let layout = vdso_api::load_and_init();
            *cached_layout = Some(layout.clone());
            layout
        }
    };

    let user_base = vdso_api::load_user(&layout) as usize;
    if user_base == 0 {
        return Err(AlienError::EINVAL);
    }

    // 把共享快照写回 vVAR 区域，让用户态时间读取能直接命中 vDSO 数据页。
    let task = current_task().unwrap();
    let mut address_space = task.address_space.lock();
    let seq_off = offset_of!(VvarData, seq);
    let realtime_off = offset_of!(VvarData, realtime_ns);
    let monotonic_off = offset_of!(VvarData, monotonic_ns);
    let realtime = time::wall_time_nanos() as usize;
    let monotonic = time::monotonic_time_nanos() as usize;
	// 先写共享快照，再把 seq 复位为 0，表示用户态可以安全读取这组时间值了。
    address_space
        .write_value_atomic(VirtAddr::from(user_base + realtime_off), realtime)
        .map_err(paging_err_to_alien)?;
    address_space
        .write_value_atomic(VirtAddr::from(user_base + monotonic_off), monotonic)
        .map_err(paging_err_to_alien)?;
    address_space
        .write_value_atomic(VirtAddr::from(user_base + seq_off), 0)
        .map_err(paging_err_to_alien)?;

    let vdso_offset = layout
        .regions
        .get(1)
        .map(|region| region.0 - layout.kernel_base)
        .unwrap_or(0);

    Ok(user_base + vdso_offset)
}

fn paging_err_to_alien(err: PagingError) -> AlienError {
    match err {
        PagingError::Mapped => AlienError::EINVAL,
        _ => AlienError::EFAULT,
    }
}
