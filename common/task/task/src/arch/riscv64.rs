use alloc::vec::Vec;

use basic::{
    AlienResult,
    task::{TaskContext, TrapFrame},
};
use ptable::VmSpace;
use xmas_elf::{ElfFile, sections::SectionData, symbol_table::Entry};

use super::UserArchState;
use crate::elf::VmmPageAllocator;

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
        _ => return Err(basic::AlienError::EINVAL),
    };
    let dynsym = match elf
        .find_section_by_name(".dynsym")
        .unwrap()
        .get_data(elf)
        .unwrap()
    {
        SectionData::DynSymbolTable64(dsym) => dsym,
        _ => return Err(basic::AlienError::EINVAL),
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
                        .map_err(|_| basic::AlienError::EINVAL)?;
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
        .ok_or(basic::AlienError::EINVAL)?
        .get_data(elf)
        .map_err(|_| basic::AlienError::EINVAL)?;
    let entries = match data {
        SectionData::Rela64(entries) => entries,
        _ => return Err(basic::AlienError::EINVAL),
    };
    let dynsym = match elf
        .find_section_by_name(".dynsym")
        .unwrap()
        .get_data(elf)
        .unwrap()
    {
        SectionData::DynSymbolTable64(dsym) => dsym,
        _ => return Err(basic::AlienError::EINVAL),
    };

    const REL_PLT: u32 = 5;

    for entry in entries.iter() {
        match entry.get_type() {
            REL_PLT => {
                let dynsym = &dynsym[entry.get_symbol_table_index() as usize];
                let symval = if dynsym.shndx() == 0 {
                    let name = dynsym
                        .get_name(elf)
                        .map_err(|_| basic::AlienError::EINVAL)?;
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

pub(super) fn arch_load_vdso() -> AlienResult<usize> {
	Ok(0)
}
