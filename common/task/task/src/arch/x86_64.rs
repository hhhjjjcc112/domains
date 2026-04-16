use alloc::{boxed::Box, vec::Vec};

use basic::{
    AlienError, AlienResult,
    config::{FRAME_SIZE, PERCPU_MIRROR_BASE},
    task::{TaskContext, TrapFrame},
    vaddr_to_paddr_in_kernel,
    vm::frame::FrameTracker,
};
use page_table::{MappingFlags, PagingError};
use ptable::{PhysPage, VmArea, VmAreaType, VmSpace};
use xmas_elf::{ElfFile, sections::SectionData, symbol_table::Entry};

use super::UserArchState;
use crate::elf::{FrameTrackerWrapper, VmmPageAllocator};

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
