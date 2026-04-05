#![no_std]
#![forbid(unsafe_code)]

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

use basic::sync::Mutex;
use basic::{println, AlienResult};
#[cfg(target_arch = "riscv64")]
use interface::define_unwind_for_APICDomain;
use interface::{APICDomain, Basic, DeviceBase};
use shared_heap::DVec;

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

#[derive(Debug, Default)]
pub struct APICDomainImpl {
    table: Mutex<BTreeMap<usize, DeviceDomain>>,
    count: Mutex<BTreeMap<usize, usize>>,
}

impl Basic for APICDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl APICDomain for APICDomainImpl {
    fn init(&self) -> AlienResult<()> {
        println!("APIC domain init");
        Ok(())
    }

    fn handle_irq(&self, irq: usize) -> AlienResult<()> {
        let mut table = self.table.lock();
        let Some(device_domain) = table.get(&irq) else {
            println!("APIC unhandled irq {}", irq);
            return Ok(());
        };

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

        let mut count = self.count.lock();
        *count.entry(irq).or_insert(0) += 1;
        Ok(())
    }

    fn register_irq(&self, irq: usize, device_domain_name: &DVec<u8>) -> AlienResult<()> {
        println!("APIC enable irq {}", irq);
        let mut table = self.table.lock();
        let device_domain_name = core::str::from_utf8(device_domain_name.as_slice()).unwrap();
        table.insert(irq, DeviceDomain::Name(device_domain_name.to_string()));
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

#[cfg(target_arch = "riscv64")]
define_unwind_for_APICDomain!(APICDomainImpl);

pub fn main() -> Box<dyn APICDomain> {
    #[cfg(target_arch = "riscv64")]
    {
        Box::new(UnwindWrap::new(APICDomainImpl::default()))
    }
    #[cfg(target_arch = "x86_64")]
    {
        Box::new(APICDomainImpl::default())
    }
}
