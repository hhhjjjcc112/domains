#![no_std]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "x86_64"))]
compile_error!("local_apic domain 仅支持 x86_64");

extern crate alloc;

use alloc::boxed::Box;
use core::{fmt, sync::atomic::{AtomicU64, Ordering}, time::Duration};

use basic::sync::Mutex;
use basic::{AlienError, AlienResult};
use basic::time::{busy_wait, current_ticks, ticks_to_nanos};
use interface::{
    define_unwind_for_LocalAPICDomain, Basic, LocalAPICDomain, LocalAPICHooks,
};
use raw_cpuid::CpuId;
use x86_apic::LocalApicContext;

static APIC_TIMER_FREQUENCY: AtomicU64 = AtomicU64::new(0);

pub struct LocalAPICDomainImpl {
    apic: Mutex<Option<LocalApicContext>>,
}

impl fmt::Debug for LocalAPICDomainImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LocalAPICDomainImpl")
    }
}

impl Default for LocalAPICDomainImpl {
    fn default() -> Self {
        Self {
            apic: Mutex::new(None),
        }
    }
}

impl Basic for LocalAPICDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

fn cpu_has_x2apic() -> bool {
    CpuId::new()
        .get_feature_info()
        .map_or(false, |finfo| finfo.has_x2apic())
}

fn raw_apic_id(cpu_id: u8) -> u32 {
    if cpu_has_x2apic() {
        cpu_id as u32
    } else {
        (cpu_id as u32) << 24
    }
}

fn program_oneshot_timer(local_apic: &mut LocalApicContext) -> AlienResult<()> {
    local_apic.configure_oneshot_timer().map_err(|_| AlienError::EINVAL)
}

fn calibrate_apic_timer(local_apic: &mut LocalApicContext) -> AlienResult<u64> {
    program_oneshot_timer(local_apic)?;
    local_apic.set_timer_initial(u32::MAX).map_err(|_| AlienError::EINVAL)?;

    busy_wait(Duration::from_millis(10));

    let remaining = local_apic.timer_current().map_err(|_| AlienError::EINVAL)?;
    let elapsed = u32::MAX.saturating_sub(remaining);
    let frequency = ((elapsed as u64) * 100).max(1);
    APIC_TIMER_FREQUENCY.store(frequency, Ordering::SeqCst);

    local_apic.set_timer_initial(0).map_err(|_| AlienError::EINVAL)?;
    Ok(frequency)
}

fn with_apic<R>(apic: &Mutex<Option<LocalApicContext>>, f: impl FnOnce(&mut LocalApicContext) -> AlienResult<R>) -> AlienResult<R> {
    let mut guard = apic.lock();
    let Some(ctx) = guard.as_mut() else {
        return Err(AlienError::EINVAL);
    };
    f(ctx)
}

impl LocalAPICDomain for LocalAPICDomainImpl {
    fn init(&self, hooks: &LocalAPICHooks) -> AlienResult<()> {
        if self.apic.lock().is_none() {
            let mut local_apic = LocalApicContext::new(hooks.xapic_base, cpu_has_x2apic())
                .map_err(|_| AlienError::EINVAL)?;
            local_apic.enable().map_err(|_| AlienError::EINVAL)?;
            let frequency = calibrate_apic_timer(&mut local_apic)?;
            program_oneshot_timer(&mut local_apic)?;
            *self.apic.lock() = Some(local_apic);
            let _ = frequency;
        }
        Ok(())
    }

    fn set_timer(&self, next_deadline: usize) -> AlienResult<()> {
        let frequency = APIC_TIMER_FREQUENCY.load(Ordering::Acquire);
        let current_tsc = current_ticks();
        let next_deadline = next_deadline as u64;
        let delta_tsc = next_deadline.saturating_sub(current_tsc);
        if frequency == 0 {
            return Err(AlienError::EINVAL);
        }

        let delta_ns = if delta_tsc == 0 {
            1
        } else {
            ticks_to_nanos(delta_tsc)
        };
        let ticks = ((delta_ns as u128 * frequency as u128 / 1_000_000_000) as u32).max(1);

        with_apic(&self.apic, |local_apic| {
            local_apic.configure_oneshot_timer().map_err(|_| AlienError::EINVAL)?;
            local_apic.set_timer_initial(ticks).map_err(|_| AlienError::EINVAL)?;
            Ok(())
        })
    }

    fn eoi(&self) -> AlienResult<()> {
        with_apic(&self.apic, |local_apic| {
            local_apic.end_of_interrupt().map_err(|_| AlienError::EINVAL)
        })
    }

    fn send_ipi(&self, target_cpu: usize, vector: u8) -> AlienResult<()> {
        with_apic(&self.apic, |local_apic| {
            let apic_id = raw_apic_id(target_cpu as u8);
            local_apic.send_ipi(apic_id, vector).map_err(|_| AlienError::EINVAL)
        })
    }

    fn send_ipi_self(&self, vector: u8) -> AlienResult<()> {
        with_apic(&self.apic, |local_apic| {
            local_apic.send_ipi_self(vector).map_err(|_| AlienError::EINVAL)
        })
    }

    fn send_ipi_all_excluding_self(&self, vector: u8) -> AlienResult<()> {
        with_apic(&self.apic, |local_apic| {
            local_apic
                .send_ipi_all_excluding_self(vector)
                .map_err(|_| AlienError::EINVAL)
        })
    }

    fn get_error_status(&self) -> AlienResult<u32> {
        with_apic(&self.apic, |local_apic| {
            local_apic.read_error_status().map_err(|_| AlienError::EINVAL)
        })
    }
}

define_unwind_for_LocalAPICDomain!(LocalAPICDomainImpl);

pub fn main() -> Box<dyn LocalAPICDomain> {
    Box::new(UnwindWrap::new(LocalAPICDomainImpl::default()))
}
