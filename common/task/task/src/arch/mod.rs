use alloc::vec::Vec;

use basic::{
    AlienResult,
    task::{TaskContext, TrapFrame},
};
use ptable::VmSpace;
use xmas_elf::ElfFile;

use crate::elf::VmmPageAllocator;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct UserArchState {
    primary: usize,
    secondary: usize,
}

impl UserArchState {
    pub const fn new(primary: usize, secondary: usize) -> Self {
        Self { primary, secondary }
    }

    pub const fn primary(self) -> usize {
        self.primary
    }

    pub const fn secondary(self) -> usize {
        self.secondary
    }
}

#[cfg(target_arch = "riscv64")]
mod riscv64;
#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(target_arch = "riscv64")]
pub(crate) use self::riscv64::*;
#[cfg(target_arch = "x86_64")]
pub(crate) use self::x86_64::*;

pub(crate) fn initial_user_state(tls: usize) -> UserArchState {
    arch_initial_user_state(tls)
}

pub(crate) fn current_user_state() -> AlienResult<UserArchState> {
    arch_current_user_state()
}

pub(crate) fn clone_user_state(
    parent_state: UserArchState,
    set_tls: bool,
    tls: usize,
) -> UserArchState {
    arch_clone_user_state(parent_state, set_tls, tls)
}

pub(crate) fn apply_user_state(context: &mut TaskContext, state: UserArchState) {
    arch_apply_user_state(context, state)
}

pub(crate) fn set_current_user_state(state: UserArchState) -> AlienResult<()> {
    arch_set_current_user_state(state)
}

pub(crate) fn apply_trap_tls(trap_frame: &mut TrapFrame, tls: usize) {
    arch_apply_trap_tls(trap_frame, tls)
}

pub(crate) fn validate_interp_path(path: &str) {
    arch_validate_interp_path(path)
}

pub(crate) fn relocate_dyn(elf: &ElfFile<'_>, bias: usize) -> AlienResult<Vec<(usize, usize)>> {
    arch_relocate_dyn(elf, bias)
}

pub(crate) fn relocate_plt(elf: &ElfFile<'_>, bias: usize) -> AlienResult<Vec<(usize, usize)>> {
    arch_relocate_plt(elf, bias)
}

pub(crate) fn map_extra_user_regions(
    address_space: &mut VmSpace<VmmPageAllocator>,
) -> AlienResult<()> {
    arch_map_extra_user_regions(address_space)
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn set_current_user_fs_base(fs_base: usize) -> AlienResult<()> {
    arch_set_current_user_fs_base(fs_base)
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn current_user_fs_base() -> AlienResult<usize> {
    arch_current_user_fs_base()
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn set_current_user_gs_base(gs_base: usize) -> AlienResult<()> {
    arch_set_current_user_gs_base(gs_base)
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn current_user_gs_base() -> AlienResult<usize> {
    arch_current_user_gs_base()
}
