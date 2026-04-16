use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use basic::{AlienError, AlienResult, config::*, vm::frame::FrameTracker};
use memory_addr::{PhysAddr, VirtAddr};
#[cfg(target_arch = "riscv64")]
use page_table::Rv64PTE as ArchPTE;
#[cfg(target_arch = "x86_64")]
use page_table::X64PTE as ArchPTE;
use page_table::{MappingFlags, NotLeafPage, PagingError, PagingIf};
use ptable::*;
use xmas_elf::{
    ElfFile,
    program::{SegmentData, Type},
};

use crate::{arch as task_arch, vfs_shim};

#[derive(Debug)]
pub struct FrameTrackerWrapper(pub(crate) FrameTracker);
impl NotLeafPage<ArchPTE> for FrameTrackerWrapper {
    fn phys_addr(&self) -> PhysAddr {
        self.0.start_phy_addr()
    }

    fn virt_addr(&self) -> VirtAddr {
        self.0.start_virt_addr()
    }

    fn zero(&self) {
        self.0.clear();
    }

    fn as_pte_slice<'a>(&self) -> &'a [ArchPTE] {
        self.0.as_slice_with(0)
    }

    fn as_pte_mut_slice<'a>(&self) -> &'a mut [ArchPTE] {
        self.0.as_mut_slice_with(0)
    }
}

impl From<FrameTracker> for FrameTrackerWrapper {
    fn from(val: FrameTracker) -> Self {
        FrameTrackerWrapper(val)
    }
}

impl Deref for FrameTrackerWrapper {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FrameTrackerWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PhysPage for FrameTrackerWrapper {
    fn phys_addr(&self) -> PhysAddr {
        self.0.start_phy_addr()
    }

    fn as_bytes(&self) -> &[u8] {
        self.0.as_slice_with(0)
    }

    fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.0.as_mut_slice_with(0)
    }

    fn read_value_atomic(&self, offset: usize) -> usize {
        self.0.read_value_atomic(offset)
    }

    fn write_value_atomic(&mut self, offset: usize, value: usize) {
        self.0.write_value_atomic(offset, value)
    }
}

#[derive(Debug)]
pub struct VmmPageAllocator;

impl PagingIf<ArchPTE> for VmmPageAllocator {
    fn alloc_frame() -> Option<Box<dyn NotLeafPage<ArchPTE>>> {
        let frame = FrameTracker::new(1);
        Some(Box::new(FrameTrackerWrapper(frame)))
    }
}

pub struct ELFInfo {
    pub address_space: VmSpace<VmmPageAllocator>,
    pub entry: VirtAddr,
    pub stack_top: VirtAddr,
    pub heap_bottom: VirtAddr,
    pub ph_num: usize,
    pub ph_entry_size: usize,
    pub ph_drift: usize,
    pub tls: usize,
    pub bias: usize,
    pub name: String,
}

pub fn calculate_bias(elf: &ElfFile) -> AlienResult<usize> {
    let bias = match elf.header.pt2.type_().as_type() {
        // static
        xmas_elf::header::Type::Executable => 0,
        xmas_elf::header::Type::SharedObject => {
            match elf
                .program_iter()
                .filter(|ph| ph.get_type().unwrap() == Type::Interp)
                .count()
            {
                // It's a loader!
                0 => ELF_BASE_RELOCATE,
                // It's a dynamically linked ELF.
                1 => 0,
                // Emmm, It has multiple interpreters.
                _ => return Err(AlienError::ENOSYS),
            }
        }
        _ => return Err(AlienError::ENOSYS),
    };
    trace!("bias: {:#x}", bias);
    Ok(bias)
}

struct LoadInfo {
    start_vaddr: usize,
    end_vaddr: usize,
    permission: MappingFlags,
    offset: usize,
    file_size: usize,
}

fn collect_load_info(elf: &ElfFile, bias: usize) -> Vec<LoadInfo> {
    let mut info = vec![];
    elf.program_iter()
        .filter(|ph| ph.get_type() == Ok(Type::Load))
        .for_each(|ph| {
            let start_addr = ph.virtual_addr() as usize + bias;
            let end_addr = start_addr + ph.mem_size() as usize;
            let mut permission: MappingFlags = MappingFlags::USER;
            let ph_flags = ph.flags();
            if ph_flags.is_read() {
                permission |= MappingFlags::READ;
            }
            if ph_flags.is_write() {
                permission |= MappingFlags::WRITE;
            }
            if ph_flags.is_execute() {
                permission |= MappingFlags::EXECUTE;
            }
            let load_info = LoadInfo {
                start_vaddr: start_addr,
                end_vaddr: end_addr,
                permission,
                offset: ph.offset() as usize,
                file_size: ph.file_size() as usize,
            };
            info.push(load_info);
        });
    info
}

#[inline]
fn paging_err_to_alien(err: PagingError) -> AlienError {
    match err {
        PagingError::NoMemory => AlienError::ENOMEM,
        _ => AlienError::EINVAL,
    }
}

pub fn load_to_vm_space(
    elf: &ElfFile,
    bias: usize,
    address_space: &mut VmSpace<VmmPageAllocator>,
) -> AlienResult<usize> {
    let mut break_addr = 0usize;
    let info = collect_load_info(elf, bias);

    for (index, section) in info.into_iter().enumerate() {
        let vaddr = VirtAddr::from(section.start_vaddr)
            .align_down_4k()
            .as_usize();
        let end_vaddr = VirtAddr::from(section.end_vaddr).align_up_4k().as_usize();
        break_addr = break_addr.max(section.end_vaddr);
        let total_pages = (end_vaddr - vaddr) / FRAME_SIZE;
        warn!(
            "[elf-load-v2] load segment: {:#x} - {:#x} -> {:#x}-{:#x}, permission: {:?}",
            section.start_vaddr, section.end_vaddr, vaddr, end_vaddr, section.permission
        );

        if section.file_size > section.end_vaddr.saturating_sub(section.start_vaddr) {
            error!(
                "segment {} invalid size: file_size={:#x}, mem_size={:#x}",
                index,
                section.file_size,
                section.end_vaddr.saturating_sub(section.start_vaddr)
            );
            return Err(AlienError::EINVAL);
        }

        let data_start = section.offset;
        let data_end = section.offset.saturating_add(section.file_size);
        if data_end > elf.input.len() {
            error!(
                "segment {} out of bound: [{:#x}, {:#x}) > elf_len={:#x}",
                index,
                data_start,
                data_end,
                elf.input.len()
            );
            return Err(AlienError::EINVAL);
        }

        let data = &elf.input[data_start..data_end];
        let seg_file_end = section.start_vaddr.saturating_add(section.file_size);
        let mut mapped_pages = 0usize;
        let mut reused_pages = 0usize;
        let mut copied = 0usize;

        let mut page = vaddr;
        while page < end_vaddr {
            match address_space.query(page) {
                Ok((_, old_permission, _)) => {
                    reused_pages += 1;
                    let merged_permission = old_permission | section.permission;
                    if merged_permission != old_permission {
                        address_space
                            .protect(page..(page + FRAME_SIZE), merged_permission)
                            .map_err(paging_err_to_alien)?;
                    }
                }
                Err(PagingError::NotMapped) => {
                    let frame = FrameTracker::new(1);
                    let area = VmArea::new(
                        page..(page + FRAME_SIZE),
                        section.permission,
                        vec![Box::new(FrameTrackerWrapper(frame))],
                    );
                    address_space.map(VmAreaType::VmArea(area)).map_err(|err| {
                        error!("segment {} map page {:#x} failed: {:?}", index, page, err);
                        paging_err_to_alien(err)
                    })?;
                    mapped_pages += 1;
                }
                Err(err) => {
                    error!("segment {} query page {:#x} failed: {:?}", index, page, err);
                    return Err(paging_err_to_alien(err));
                }
            }

            let copy_start = page.max(section.start_vaddr);
            let copy_end = (page + FRAME_SIZE).min(seg_file_end);
            if copy_start < copy_end {
                let src_offset = copy_start - section.start_vaddr;
                let len = copy_end - copy_start;
                address_space
                    .write_bytes(
                        VirtAddr::from(copy_start),
                        &data[src_offset..(src_offset + len)],
                    )
                    .map_err(|err| {
                        error!(
                            "segment {} write page data failed: va={:#x}, len={:#x}, err={:?}",
                            index, copy_start, len, err
                        );
                        paging_err_to_alien(err)
                    })?;
                copied += len;
            }

            page += FRAME_SIZE;
        }

        if copied != section.file_size {
            error!(
                "segment {} copied size mismatch: copied={:#x}, file_size={:#x}",
                index, copied, section.file_size
            );
            return Err(AlienError::EINVAL);
        }

        warn!(
            "[elf-load-v2] segment {} done: new_pages={}, reused_pages={}, copied={:#x}, total_pages={}",
            index, mapped_pages, reused_pages, copied, total_pages
        );
    }

    Ok(break_addr)
}

fn relocate_dyn(elf: &ElfFile<'_>, bias: usize) -> AlienResult<Vec<(usize, usize)>> {
    task_arch::relocate_dyn(elf, bias)
}

fn relocate_plt(elf: &ElfFile<'_>, bias: usize) -> AlienResult<Vec<(usize, usize)>> {
    task_arch::relocate_plt(elf, bias)
}

pub fn build_vm_space(elf: &[u8], args: &mut Vec<String>, name: &str) -> AlienResult<ELFInfo> {
    let elf = xmas_elf::ElfFile::new(elf).map_err(|_| AlienError::EINVAL)?;
    // if the elf file is a shared object, we should load the interpreter first
    if let Some(inter) = elf
        .program_iter()
        .find(|ph| ph.get_type().unwrap() == Type::Interp)
    {
        let data = match inter.get_data(&elf).unwrap() {
            SegmentData::Undefined(data) => data,
            _ => return Err(AlienError::EINVAL),
        };
        let path = core::str::from_utf8(data).unwrap();
        task_arch::validate_interp_path(path);
        let mut new_args = vec!["/libc.so\0".to_string()];
        new_args.extend(args.clone());
        *args = new_args;
        // load interpreter
        let mut data = vec![];
        info!("load interpreter: {}, new_args:{:?}", path, args);
        if vfs_shim::read_all("libc.so", &mut data) {
            return build_vm_space(&data, args, "libc.so");
        } else {
            panic!(
                "[build_vm_space] Found interpreter path: {}， but read error",
                path
            );
        }
    };

    let bias = calculate_bias(&elf)?;

    let tls = elf
        .program_iter()
        .find(|x| x.get_type().unwrap() == Type::Tls)
        .map(|ph| ph.virtual_addr())
        .unwrap_or(bias as u64);
    info!("ELF tls: {:#x}", tls);

    let mut address_space = VmSpace::new();
    let break_addr = load_to_vm_space(&elf, bias, &mut address_space)?;

    // user stack
    let ceil_addr = PhysAddr::from(break_addr + FRAME_SIZE)
        .align_up_4k()
        .as_usize();

    let user_stack_low = ceil_addr + FRAME_SIZE;
    let uer_stack_top = user_stack_low + USER_STACK_SIZE;
    warn!("user stack: {:#x} - {:#x}", user_stack_low, uer_stack_top);

    let mut user_stack_phy_frames: Vec<Box<dyn PhysPage>> = vec![];
    for _ in 0..USER_STACK_SIZE / FRAME_SIZE {
        let frame = FrameTracker::new(1);
        user_stack_phy_frames.push(Box::new(FrameTrackerWrapper(frame)));
    }
    let user_stack_area = VmArea::new(
        user_stack_low..uer_stack_top,
        MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE,
        user_stack_phy_frames,
    );
    address_space
        .map(VmAreaType::VmArea(user_stack_area))
        .map_err(paging_err_to_alien)?;

    let heap_bottom = uer_stack_top;

    let trap_context_frame = FrameTracker::new(1);
    let trap_context_area = VmArea::new(
        TRAP_CONTEXT_BASE..(TRAP_CONTEXT_BASE + FRAME_SIZE),
        MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE,
        vec![Box::new(FrameTrackerWrapper(trap_context_frame))],
    );
    address_space
        .map(VmAreaType::VmArea(trap_context_area))
        .map_err(paging_err_to_alien)?;

    // todo!(how to solve trampoline)
    let trampoline_frame = FrameTracker::create_trampoline();

    let trampoline_area = VmArea::new(
        TRAMPOLINE..(TRAMPOLINE + FRAME_SIZE),
        MappingFlags::READ | MappingFlags::EXECUTE,
        vec![Box::new(FrameTrackerWrapper(trampoline_frame))],
    );
    address_space
        .map(VmAreaType::VmArea(trampoline_area))
        .map_err(paging_err_to_alien)?;

    task_arch::map_extra_user_regions(&mut address_space)?;

    let res = if let Some(phdr) = elf
        .program_iter()
        .find(|ph| ph.get_type() == Ok(Type::Phdr))
    {
        // if phdr exists in program header, use it
        Ok(phdr.virtual_addr())
    } else if let Some(elf_addr) = elf
        .program_iter()
        .find(|ph| ph.get_type() == Ok(Type::Load) && ph.offset() == 0)
    {
        // otherwise, check if elf is loaded from the beginning, then phdr can be inferred.
        // Ok(elf_addr.virtual_addr())
        Ok(elf_addr.virtual_addr() + elf.header.pt2.ph_offset())
    } else {
        warn!("elf: no phdr found, tls might not work");
        Err(AlienError::EINVAL)
    }
    .unwrap_or(0);
    warn!(
        "entry: {:#x}, phdr:{:#x}",
        elf.header.pt2.entry_point() + bias as u64,
        res + bias as u64
    );
    // todo!(relocate)
    if bias != 0 {
        // if the elf file is a shared object, we should relocate it
        if let Ok(kvs) = relocate_dyn(&elf, bias) {
            kvs.into_iter().for_each(|kv| {
                debug!("relocate: {:#x} -> {:#x}", kv.0, kv.1);
                address_space
                    .write_val(VirtAddr::from(kv.0), &kv.1)
                    .unwrap()
            });
            info!("relocate dynamic section success")
        }
        if let Ok(kvs) = relocate_plt(&elf, bias) {
            kvs.into_iter().for_each(|kv| {
                debug!("relocate: {:#x} -> {:#x}", kv.0, kv.1);
                address_space
                    .write_val(VirtAddr::from(kv.0), &kv.1)
                    .unwrap()
            });
            info!("relocate plt section success");
        }
    }

    Ok(ELFInfo {
        address_space,
        entry: VirtAddr::from(elf.header.pt2.entry_point() as usize + bias),
        stack_top: VirtAddr::from(uer_stack_top),
        heap_bottom: VirtAddr::from(heap_bottom),
        ph_num: elf.header.pt2.ph_count() as usize,
        ph_entry_size: elf.header.pt2.ph_entry_size() as usize,
        ph_drift: res as usize + bias,
        tls: tls as usize,
        bias,
        name: name.to_string(),
    })
}

pub fn clone_vm_space(vm_space: &VmSpace<VmmPageAllocator>) -> VmSpace<VmmPageAllocator> {
    let mut space = VmSpace::new();
    let trampoline_frame = FrameTracker::create_trampoline();
    let trampoline_frame_virt_addr = trampoline_frame.start_virt_addr().as_usize();
    vm_space.area_iter().for_each(|ty| match ty {
        VmAreaType::VmArea(area) => {
            let size = area.size();
            let start = area.start();
            info!("<clone_vm_space> start: {:#x}, size: {:#x}", start, size);
            if start == trampoline_frame_virt_addr {
                let trampoline_frame = FrameTracker::create_trampoline();
                let trampoline_area = VmArea::new(
                    TRAMPOLINE..(TRAMPOLINE + FRAME_SIZE),
                    MappingFlags::READ | MappingFlags::EXECUTE,
                    vec![Box::new(FrameTrackerWrapper(trampoline_frame))],
                );
                space.map(VmAreaType::VmArea(trampoline_area)).unwrap();
            } else {
                let mut phy_frames: Vec<Box<dyn PhysPage>> = vec![];
                for _ in 0..size / FRAME_SIZE {
                    let frame = FrameTracker::new(1);
                    phy_frames.push(Box::new(FrameTrackerWrapper(frame)));
                }
                let new_area = area.clone_with(phy_frames);
                space.map(VmAreaType::VmArea(new_area)).unwrap();
            }
        }
        VmAreaType::VmAreaEqual(area_eq) => {
            let new_area_eq = area_eq.clone();
            space.map(VmAreaType::VmAreaEqual(new_area_eq)).unwrap();
        }
    });
    space
}

pub fn extend_thread_vm_space(space: &mut VmSpace<VmmPageAllocator>, thread_num: usize) {
    assert!(thread_num > 0);
    let address = TRAP_CONTEXT_BASE - FRAME_SIZE * thread_num;
    let trap_context_frame = FrameTracker::new(1);
    let trap_context_area = VmArea::new(
        address..(address + FRAME_SIZE),
        MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE,
        vec![Box::new(FrameTrackerWrapper(trap_context_frame))],
    );
    space.map(VmAreaType::VmArea(trap_context_area)).unwrap();
    // copy trampoline
    let mut old_trampoline_buf = [0; FRAME_SIZE];
    space
        .read_bytes(VirtAddr::from(TRAP_CONTEXT_BASE), &mut old_trampoline_buf)
        .unwrap();
    space
        .write_bytes(VirtAddr::from(address), &old_trampoline_buf)
        .unwrap();
}
