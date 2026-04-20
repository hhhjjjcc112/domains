use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::mem::offset_of;

use basic::{
    AlienError, AlienResult,
    config::{FRAME_SIZE, PERCPU_MIRROR_BASE},
    constants::io::{MMapFlags, ProtFlags},
    sync::Mutex,
    time,
    task::{TaskContext, TrapFrame},
    vaddr_to_paddr_in_kernel,
    vm::frame::FrameTracker,
};
use page_table::{MappingFlags, PagingError};
use ptable::{PhysPage, VmArea, VmAreaType, VmIo, VmSpace};
use xmas_elf::{ElfFile, sections::SectionData, symbol_table::Entry};

use super::UserArchState;
use crate::{elf::{FrameTrackerWrapper, VmmPageAllocator}, processor::current_task, resource::MMapRegion};
use vdso_api::{MappingFlags as VdsoMappingFlags, MemIf, VvarData};

#[derive(Debug)]
struct VdsoPage {
    frame: Arc<FrameTracker>,
    page_index: usize,
}

impl PhysPage for VdsoPage {
    fn phys_addr(&self) -> memory_addr::PhysAddr {
        memory_addr::PhysAddr::from(
            self.frame.start_phy_addr().as_usize() + self.page_index * FRAME_SIZE,
        )
    }

    fn as_bytes(&self) -> &[u8] {
        let start = self.page_index * FRAME_SIZE;
        // 同一组物理页同时充当 vDSO backing 和 vVAR backing，这里按页切片给页表使用。
        &self.frame.as_ref().as_slice_with::<u8>(start)[..FRAME_SIZE]
    }

    fn as_mut_bytes(&mut self) -> &mut [u8] {
        let start = self.page_index * FRAME_SIZE;
        // 内核在装载阶段需要先写入内容，所以这里必须返回可写切片。
        &mut self.frame.as_ref().as_mut_slice_with::<u8>(start)[..FRAME_SIZE]
    }

    fn read_value_atomic(&self, offset: usize) -> usize {
        self.frame.read_value_atomic(self.page_index * FRAME_SIZE + offset)
    }

    fn write_value_atomic(&mut self, offset: usize, value: usize) {
        self.frame
            .write_value_atomic(self.page_index * FRAME_SIZE + offset, value)
    }
}

#[derive(Debug)]
struct VdsoAllocState {
    kernel_base: usize,
    user_base: usize,
    _frame: Arc<FrameTracker>,
}

static VDSO_SHARED_FRAME: Mutex<Option<Arc<FrameTracker>>> = Mutex::new(None);
static VDSO_ALLOC_STATE: Mutex<Option<VdsoAllocState>> = Mutex::new(None);

fn page_align(size: usize) -> usize {
	(size + FRAME_SIZE - 1) & !(FRAME_SIZE - 1)
}

pub(super) fn arch_initial_user_state(tls: usize) -> UserArchState {
    UserArchState::new(tls, 0)
}

pub(super) fn arch_current_user_state() -> AlienResult<UserArchState> {
    Ok(UserArchState::new(
        basic::current_user_fs_base()?,
        basic::current_user_gs_base()?,
    ))
}

pub(super) fn arch_clone_user_state(
    parent_state: UserArchState,
    set_tls: bool,
    tls: usize,
) -> UserArchState {
    if set_tls {
        UserArchState::new(tls, parent_state.secondary())
    } else {
        parent_state
    }
}

pub(super) fn arch_apply_user_state(context: &mut TaskContext, state: UserArchState) {
    context.set_fs_base(state.primary());
    context.set_gs_base(state.secondary());
}

pub(super) fn arch_set_current_user_state(state: UserArchState) -> AlienResult<()> {
    basic::set_current_user_fs_base(state.primary())?;
    basic::set_current_user_gs_base(state.secondary())?;
    Ok(())
}

pub(super) fn arch_set_current_user_fs_base(fs_base: usize) -> AlienResult<()> {
    basic::set_current_user_fs_base(fs_base)
}

pub(super) fn arch_current_user_fs_base() -> AlienResult<usize> {
    basic::current_user_fs_base()
}

pub(super) fn arch_set_current_user_gs_base(gs_base: usize) -> AlienResult<()> {
    basic::set_current_user_gs_base(gs_base)
}

pub(super) fn arch_current_user_gs_base() -> AlienResult<usize> {
    basic::current_user_gs_base()
}

pub(super) fn arch_apply_trap_tls(_trap_frame: &mut TrapFrame, _tls: usize) {}

pub(super) fn arch_validate_interp_path(path: &str) {
    assert!(path.starts_with("/lib/ld-musl-x86_64"));
}

pub(super) fn arch_relocate_dyn(
    elf: &ElfFile<'_>,
    bias: usize,
) -> AlienResult<Vec<(usize, usize)>> {
    let mut res = Vec::new();
    let data = elf
        .find_section_by_name(".rela.dyn")
        .unwrap()
        .get_data(elf)
        .unwrap();
    let entries = match data {
        SectionData::Rela64(entries) => entries,
        _ => return Err(AlienError::EINVAL),
    };
    let dynsym = match elf
        .find_section_by_name(".dynsym")
        .unwrap()
        .get_data(elf)
        .unwrap()
    {
        SectionData::DynSymbolTable64(dsym) => dsym,
        _ => return Err(AlienError::EINVAL),
    };

    const REL_SYM_ABS: u32 = 1;
    const REL_GOT: u32 = 6;
    const REL_PLT: u32 = 7;
    const REL_RELATIVE: u32 = 8;

    for entry in entries.iter() {
        match entry.get_type() {
            REL_SYM_ABS | REL_GOT | REL_PLT => {
                let dynsym = &dynsym[entry.get_symbol_table_index() as usize];
                let symval = if dynsym.shndx() == 0 {
                    let name = dynsym.get_name(elf).map_err(|_| AlienError::EINVAL)?;
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
    elf: &ElfFile<'_>,
    bias: usize,
) -> AlienResult<Vec<(usize, usize)>> {
    let mut res = Vec::new();
    let data = elf
        .find_section_by_name(".rela.plt")
        .ok_or(AlienError::EINVAL)?
        .get_data(elf)
        .map_err(|_| AlienError::EINVAL)?;
    let entries = match data {
        SectionData::Rela64(entries) => entries,
        _ => return Err(AlienError::EINVAL),
    };
    let dynsym = match elf
        .find_section_by_name(".dynsym")
        .unwrap()
        .get_data(elf)
        .unwrap()
    {
        SectionData::DynSymbolTable64(dsym) => dsym,
        _ => return Err(AlienError::EINVAL),
    };

    const REL_PLT: u32 = 7;

    for entry in entries.iter() {
        match entry.get_type() {
            REL_PLT => {
                let dynsym = &dynsym[entry.get_symbol_table_index() as usize];
                let symval = if dynsym.shndx() == 0 {
                    let name = dynsym.get_name(elf).map_err(|_| AlienError::EINVAL)?;
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

fn paging_err_to_alien(err: PagingError) -> AlienError {
    match err {
        PagingError::NoMemory => AlienError::ENOMEM,
        _ => AlienError::EINVAL,
    }
}

pub(super) fn arch_map_extra_user_regions(
    address_space: &mut VmSpace<VmmPageAllocator>,
) -> AlienResult<()> {
    const PERCPU_USER_MAP_SIZE: usize = 0x20_000;
    const KERNEL_CORE_MAP_START: usize = 0x20_0000;
    const KERNEL_CORE_MAP_SIZE: usize = 0xA0_0000;

    let mut percpu_frames: Vec<Box<dyn PhysPage>> = Vec::new();
    for off in (0..PERCPU_USER_MAP_SIZE).step_by(FRAME_SIZE) {
        let va = PERCPU_MIRROR_BASE + off;
        let pa = match vaddr_to_paddr_in_kernel(va) {
            Ok(pa) => pa,
            Err(err) => {
                if off == 0 {
                    warn!("skip percpu mirror map: va={:#x}, err={:?}", va, err);
                }
                break;
            }
        };
        let frame = FrameTracker::from_phy_range(pa..(pa + FRAME_SIZE));
        percpu_frames.push(Box::new(FrameTrackerWrapper(frame)));
    }
    if !percpu_frames.is_empty() {
        let mapped_size = percpu_frames.len() * FRAME_SIZE;
        let percpu_area = VmArea::new(
            PERCPU_MIRROR_BASE..(PERCPU_MIRROR_BASE + mapped_size),
            MappingFlags::READ | MappingFlags::WRITE,
            percpu_frames,
        );
        address_space
            .map(VmAreaType::VmArea(percpu_area))
            .map_err(paging_err_to_alien)?;
    }

    let mut core_frames: Vec<Box<dyn PhysPage>> = Vec::new();
    for off in (0..KERNEL_CORE_MAP_SIZE).step_by(FRAME_SIZE) {
        let va = KERNEL_CORE_MAP_START + off;
        let pa = match vaddr_to_paddr_in_kernel(va) {
            Ok(pa) => pa,
            Err(_) => break,
        };
        let frame = FrameTracker::from_phy_range(pa..(pa + FRAME_SIZE));
        core_frames.push(Box::new(FrameTrackerWrapper(frame)));
    }
    if !core_frames.is_empty() {
        let mapped_size = core_frames.len() * FRAME_SIZE;
        let area = VmArea::new(
            KERNEL_CORE_MAP_START..(KERNEL_CORE_MAP_START + mapped_size),
            MappingFlags::READ | MappingFlags::WRITE,
            core_frames,
        );
        address_space
            .map(VmAreaType::VmArea(area))
            .map_err(paging_err_to_alien)?;
    }

    Ok(())
}

struct KernelVdsoMem;

#[crate_interface::impl_interface]
impl MemIf for KernelVdsoMem {
    fn alloc(size: usize) -> *mut u8 {
        let task = current_task().unwrap();
        assert_eq!(size % FRAME_SIZE, 0);
        let page_count = size / FRAME_SIZE;
        let frame = {
            let mut shared_frame = VDSO_SHARED_FRAME.lock();
            if let Some(frame) = shared_frame.as_ref() {
                Arc::clone(frame)
            } else {
                // 内核只初始化一次 backing，后续 exec 直接复用同一份物理页。
                let frame = Arc::new(FrameTracker::new(page_count));
                frame.clear();
                *shared_frame = Some(Arc::clone(&frame));
                frame
            }
        };
        let kernel_base = frame.start_virt_addr().as_usize();

        let mut mmap = task.mmap.lock();
        let v_range = mmap.alloc(size);
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

        let mut phy_frames: Vec<Box<dyn PhysPage>> = Vec::new();
        for page_index in 0..page_count {
            phy_frames.push(Box::new(VdsoPage {
                frame: frame.clone(),
                page_index,
            }));
        }
        let area = VmArea::new(
            v_range.clone(),
            MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER,
            phy_frames,
        );
        task.address_space
            .lock()
            .map(VmAreaType::VmArea(area))
            .unwrap();

        // 记住 kernel backing 和用户映射的对应关系，后续 `protect()` 需要把地址换回用户视角。
        *VDSO_ALLOC_STATE.lock() = Some(VdsoAllocState {
            kernel_base,
            user_base: v_range.start,
            _frame: frame,
        });

        kernel_base as *mut u8
    }

    fn protect(addr: *mut u8, len: usize, flags: VdsoMappingFlags) {
        let len = page_align(len);
        let (kernel_base, user_base) = {
            let state = VDSO_ALLOC_STATE.lock();
            let Some(state) = state.as_ref() else {
                return;
            };
            (state.kernel_base, state.user_base)
        };

        // loader 传进来的地址是 kernel backing 地址，这里换算成用户映射地址后再收紧权限。
        let user_addr = user_base + (addr as usize).saturating_sub(kernel_base);
        let task = current_task().unwrap();
        let mut mmap = task.mmap.lock();
        if let Some(region) = mmap.get_region_mut(user_addr) {
            region.set_prot(ProtFlags::from_bits_truncate(flags.bits() as u32));
        }

        let map_flags = MappingFlags::from_bits_truncate(flags.bits());
        task.address_space
            .lock()
            .protect(user_addr..(user_addr + len), map_flags)
            .unwrap();
    }
}

pub(super) fn arch_load_vdso() -> AlienResult<usize> {
    // 内核不把 vDSO 当作普通内核模块加载；这里只是为当前任务准备用户映射、
    // 写入共享时间快照，并把用户可见基址交给 auxv。
    let regions = vdso_api::load_and_init();
    if regions.len() < 2 {
        return Err(AlienError::EINVAL);
    }

    let (kernel_base, user_base) = {
        let state = VDSO_ALLOC_STATE.lock();
        let Some(state) = state.as_ref() else {
            return Err(AlienError::EINVAL);
        };
        (regions[0].0 as usize, state.user_base)
    };
    // loader 返回的是内核可写 backing 的地址，这里要换回用户映射基址。
    let vdso_offset = regions[1].0 as usize - kernel_base;
    let task = current_task().unwrap();
    let mut address_space = task.address_space.lock();
    let seq_off = offset_of!(VvarData, seq);
    let realtime_off = offset_of!(VvarData, realtime_ns);
    let monotonic_off = offset_of!(VvarData, monotonic_ns);
    let realtime = time::wall_time_nanos() as usize;
    let monotonic = time::monotonic_time_nanos() as usize;
	// 先写共享快照，再把 seq 复位为 0，表示用户态可以安全读取这组时间值了。
    address_space
        .write_value_atomic(memory_addr::VirtAddr::from(user_base + realtime_off), realtime)
        .map_err(paging_err_to_alien)?;
    address_space
        .write_value_atomic(memory_addr::VirtAddr::from(user_base + monotonic_off), monotonic)
        .map_err(paging_err_to_alien)?;
    address_space
        .write_value_atomic(memory_addr::VirtAddr::from(user_base + seq_off), 0)
        .map_err(paging_err_to_alien)?;

    // 返回给 exec/auxv 的必须是用户态可见的 vDSO 入口，而不是内核 backing。
    Ok(user_base + vdso_offset)
}
