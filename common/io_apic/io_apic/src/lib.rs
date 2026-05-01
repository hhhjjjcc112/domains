#![no_std]

#[cfg(not(target_arch = "x86_64"))]
compile_error!("io_apic domain 仅支持 x86_64");

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
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
};

use basic::sync::Mutex;
use basic::{println, AlienResult};
use interface::{define_unwind_for_IoAPICDomain, Basic, DeviceBase, IoAPICDomain, IoAPICHooks};
use shared_heap::DVec;
use x2apic::ioapic::{IrqFlags, IrqMode, IoApic};

static mut IO_APIC: MaybeUninit<IoApic> = MaybeUninit::uninit();
static IO_APIC_READY: AtomicBool = AtomicBool::new(false);

unsafe fn get_io_apic() -> &'static mut IoApic {
    #[allow(static_mut_refs)]
    unsafe {
        IO_APIC.assume_init_mut()
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
pub struct IoAPICDomainImpl {
    table: Mutex<BTreeMap<usize, DeviceDomain>>,
    count: Mutex<BTreeMap<usize, usize>>,
}

impl Default for IoAPICDomainImpl {
    fn default() -> Self {
        Self {
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
        if !IO_APIC_READY.load(Ordering::Acquire) {
            let io_apic = unsafe { IoApic::new(hooks.ioapic_base as u64) };
            unsafe {
                #[allow(static_mut_refs)]
                IO_APIC.write(io_apic);
            }
            IO_APIC_READY.store(true, Ordering::Release);
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
            IO_APIC_READY.load(Ordering::Acquire)
        );
        if IO_APIC_READY.load(Ordering::Acquire) {
            unsafe {
                let io_apic = get_io_apic();
                let mut entry = io_apic.table_entry(irq);
                println!("IO APIC configure irq table_entry ok irq={}", irq);
                entry.set_vector(vector);
                entry.set_dest(dest_cpu);
                entry.set_mode(IrqMode::Fixed);
                entry.set_flags(
                    IrqFlags::LEVEL_TRIGGERED | IrqFlags::LOW_ACTIVE | IrqFlags::MASKED,
                );
                println!("IO APIC configure irq writing entry irq={}", irq);
                io_apic.set_table_entry(irq, entry);
                println!("IO APIC configure irq write ok irq={}", irq);
            }
        }
        Ok(())
    }

    fn set_irq_enable(&self, vector: usize, enabled: bool) -> AlienResult<()> {
        if vector < 0xf0 && IO_APIC_READY.load(Ordering::Acquire) {
            unsafe {
                let io_apic = get_io_apic();
                if enabled {
                    io_apic.enable_irq(vector as u8);
                } else {
                    io_apic.disable_irq(vector as u8);
                }
            }
        }
        Ok(())
    }

    fn ioapic_max_entries(&self) -> AlienResult<u8> {
        if IO_APIC_READY.load(Ordering::Acquire) {
            Ok(unsafe { get_io_apic().max_table_entry() + 1 })
        } else {
            Ok(0)
        }
    }

    fn handle_irq(&self, irq: usize) -> AlienResult<()> {
        let mut table = self.table.lock();
        let Some(device_domain) = table.get(&irq) else {
            println!("IO APIC unhandled irq {}", irq);
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
        println!("IO APIC enable irq {}", irq);
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

define_unwind_for_IoAPICDomain!(IoAPICDomainImpl);

pub fn main() -> Box<dyn IoAPICDomain> {
    Box::new(UnwindWrap::new(IoAPICDomainImpl::default()))
}
