#![no_std]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "riscv64"))]
compile_error!("plic domain 仅支持 riscv64");

extern crate alloc;

use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
};
use core::{
    cmp::min,
    fmt::{Debug, Formatter},
};

use basic::{arch, config::CPU_NUM, io::SafeIORegion, println, sync::Mutex, AlienResult};
use interface::{define_unwind_for_PLICDomain, Basic, DeviceBase, PLICDomain, PlicInfo, PlicType};
use raw_plic::{Mode, PlicIO, PLIC};
use shared_heap::DVec;
use spin::Once;

static PLIC: Once<PLIC<CPU_NUM>> = Once::new();

#[derive(Debug)]
pub struct SafeIORegionWrapper(SafeIORegion);

impl PlicIO for SafeIORegionWrapper {
    fn read_at(&self, offset: usize) -> u32 {
        self.0.read_at(offset).unwrap()
    }

    fn write_at(&self, offset: usize, value: u32) {
        self.0.write_at(offset, value).unwrap()
    }
}

enum DeviceDomain {
    Name(String),
    Domain(Arc<dyn DeviceBase>),
}

impl Debug for DeviceDomain {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            DeviceDomain::Name(name) => write!(f, "Name({})", name),
            DeviceDomain::Domain(_) => write!(f, "Domain"),
        }
    }
}

#[derive(Debug)]
pub struct PLICDomainImpl {
    table: Mutex<BTreeMap<usize, DeviceDomain>>,
    count: Mutex<BTreeMap<usize, usize>>,
}

impl Default for PLICDomainImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl PLICDomainImpl {
    pub fn new() -> Self {
        Self {
            table: Mutex::new(BTreeMap::new()),
            count: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Basic for PLICDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl PLICDomain for PLICDomainImpl {
    fn init(&self, plic_info: &PlicInfo) -> AlienResult<()> {
        println!("plic region: {:#x?}", plic_info.device_info);
        let plic_space = SafeIORegion::from(plic_info.device_info.clone());
        let privileges = match plic_info.ty {
            PlicType::Qemu => [2; CPU_NUM],
            PlicType::SiFive => {
                let mut privileges = [2; CPU_NUM];
                // core 0 don't have S mode
                privileges[0] = 1;
                privileges
            }
        };
        PLIC.call_once(|| PLIC::new(Box::new(SafeIORegionWrapper(plic_space)), privileges));
        println!("Init qemu plic success");
        Ok(())
    }

    fn handle_irq(&self) -> AlienResult<()> {
        let plic = PLIC.get().unwrap();
        let hart_id = arch::hart_id();
        let irq = plic.claim(hart_id as u32, Mode::Supervisor) as usize;
        let mut table = self.table.lock();
        let device_domain = table
            .get(&irq)
            .or_else(|| panic!("no device for irq {}", irq))
            .unwrap();

        match device_domain {
            DeviceDomain::Name(name) => {
                let device_domain = basic::get_domain(name).unwrap();
                let device_domain: Arc<dyn DeviceBase> = device_domain.try_into()?;
                device_domain.handle_irq()?;
                table.insert(irq, DeviceDomain::Domain(device_domain));
            }
            DeviceDomain::Domain(device) => {
                device.handle_irq()?;
            }
        }
        plic.complete(hart_id as u32, Mode::Supervisor, irq as u32);
        *self
            .count
            .lock()
            .get_mut(&irq)
            .or_else(|| panic!("no device for irq {}", irq))
            .unwrap() += 1;
        Ok(())
    }

    fn register_irq(&self, irq: usize, device_domain_name: &DVec<u8>) -> AlienResult<()> {
        let hard_id = arch::hart_id();
        println!(
            "PLIC enable irq {} for hart {}, priority {}",
            irq, hard_id, 1
        );
        let plic = PLIC.get().unwrap();
        plic.set_threshold(hard_id as u32, Mode::Machine, 1);
        plic.set_threshold(hard_id as u32, Mode::Supervisor, 0);
        plic.complete(hard_id as u32, Mode::Supervisor, irq as u32);
        plic.set_priority(irq as u32, 1);
        plic.enable(hard_id as u32, Mode::Supervisor, irq as u32);
        let mut table = self.table.lock();
        let device_domain_name = core::str::from_utf8(device_domain_name.as_slice()).unwrap();
        let domain = DeviceDomain::Name(device_domain_name.to_string());
        table.insert(irq, domain);
        self.count.lock().insert(irq, 0);
        Ok(())
    }

    fn irq_info(&self, mut buf: DVec<u8>) -> AlienResult<DVec<u8>> {
        let interrupts = self.count.lock();
        let mut res = String::new();
        interrupts.iter().for_each(|(irq, value)| {
            res.push_str(&format!("{}: {}\r\n", irq, value));
        });
        let copy_len = min(buf.len(), res.as_bytes().len());
        buf.as_mut_slice()[..copy_len].copy_from_slice(&res.as_bytes()[..copy_len]);
        Ok(buf)
    }
}

define_unwind_for_PLICDomain!(PLICDomainImpl);

pub fn main() -> Box<dyn PLICDomain> {
    let domain_impl = PLICDomainImpl::new();
    Box::new(UnwindWrap::new(domain_impl))
}
