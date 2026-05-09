#![no_std]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "x86_64"))]
compile_error!("io_apic domain 仅支持 x86_64");

extern crate alloc;

use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::{
    cmp::min,
    fmt::{Debug, Formatter},
};

use basic::sync::Mutex;
use basic::{println, AlienError, AlienResult};
use interface::{define_unwind_for_IoAPICDomain, Basic, DeviceBase, IoAPICDomain, IoAPICHooks};
use shared_heap::DVec;
use x86_apic::IoApicContext;

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

pub struct IoAPICDomainImpl {
    apic: Mutex<Option<IoApicContext>>,
    table: Mutex<BTreeMap<usize, Vec<DeviceDomain>>>,
    count: Mutex<BTreeMap<usize, usize>>,
}

impl Debug for IoAPICDomainImpl {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("IoAPICDomainImpl")
    }
}

impl Default for IoAPICDomainImpl {
    fn default() -> Self {
        Self {
            apic: Mutex::new(None),
            table: Mutex::new(BTreeMap::new()),
            count: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Basic for IoAPICDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl IoAPICDomain for IoAPICDomainImpl {
    fn init(&self, hooks: &IoAPICHooks) -> AlienResult<()> {
        println!("IO APIC domain init enter base={:#x}", hooks.ioapic_base);
        if self.apic.lock().is_none() {
            let io_apic = IoApicContext::new(hooks.ioapic_base)
                .map_err(|_| AlienError::EINVAL)?;
            *self.apic.lock() = Some(io_apic);
        }
        println!("IO APIC domain init");
        Ok(())
    }

    fn configure_irq(&self, irq: u8, vector: u8, dest_cpu: u8) -> AlienResult<()> {
        println!(
            "IO APIC configure irq enter irq={} vector={:#x} dest_cpu={} ready={}",
            irq,
            vector,
            dest_cpu,
            self.apic.lock().is_some()
        );
        let mut guard = self.apic.lock();
        let Some(io_apic) = guard.as_mut() else {
            return Err(AlienError::EINVAL);
        };

        io_apic
            .configure_irq(irq, vector, dest_cpu)
            .map_err(|_| AlienError::EINVAL)?;
        Ok(())
    }

    fn set_irq_enable(&self, vector: usize, enabled: bool) -> AlienResult<()> {
        if vector < 0xf0 {
            let mut guard = self.apic.lock();
            let Some(io_apic) = guard.as_mut() else {
                return Err(AlienError::EINVAL);
            };
            if enabled {
                io_apic
                    .enable_irq(vector as u8)
                    .map_err(|_| AlienError::EINVAL)?;
            } else {
                io_apic
                    .disable_irq(vector as u8)
                    .map_err(|_| AlienError::EINVAL)?;
            }
        }
        Ok(())
    }

    fn ioapic_max_entries(&self) -> AlienResult<u8> {
        let mut guard = self.apic.lock();
        if let Some(io_apic) = guard.as_mut() {
            Ok(io_apic.max_table_entry().map_err(|_| AlienError::EINVAL)? + 1)
        } else {
            Ok(0)
        }
    }

    fn handle_irq(&self, irq: usize) -> AlienResult<()> {
        let mut table = self.table.lock();
        let Some(device_domains) = table.get_mut(&irq) else {
            println!("IO APIC unhandled irq {}", irq);
            return Ok(());
        };

        for device_domain in device_domains.iter_mut() {
            match device_domain {
                DeviceDomain::Name(name) => {
                    let Some(raw_domain) = basic::get_domain(name) else {
                        println!("IO APIC irq {} missing domain {}", irq, name);
                        continue;
                    };
                    let typed_domain: Arc<dyn DeviceBase> = raw_domain.try_into()?;
                    typed_domain.handle_irq()?;
                    *device_domain = DeviceDomain::Domain(typed_domain);
                }
                DeviceDomain::Domain(device) => {
                    device.handle_irq()?;
                }
            }
        }

        let mut count = self.count.lock();
        *count.entry(irq).or_insert(0) += 1;
        Ok(())
    }

    fn register_irq(&self, irq: usize, device_domain_name: &DVec<u8>) -> AlienResult<()> {
        println!("IO APIC enable irq {}", irq);
        let mut table = self.table.lock();
        let device_domain_name = core::str::from_utf8(device_domain_name.as_slice()).unwrap();
        let entry = table.entry(irq).or_insert_with(Vec::new);
        let duplicated = entry.iter().any(|domain| match domain {
            DeviceDomain::Name(existing) => existing == device_domain_name,
            DeviceDomain::Domain(_) => false,
        });
        if !duplicated {
            entry.push(DeviceDomain::Name(device_domain_name.to_string()));
        }
        self.count.lock().entry(irq).or_insert(0);
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

define_unwind_for_IoAPICDomain!(IoAPICDomainImpl);

pub fn main() -> Box<dyn IoAPICDomain> {
    Box::new(UnwindWrap::new(IoAPICDomainImpl::default()))
}
