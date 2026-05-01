#![no_std]

#[cfg(not(target_arch = "x86_64"))]
compile_error!("local_apic domain 仅支持 x86_64");

extern crate alloc;

use alloc::boxed::Box;
use core::{
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

use basic::sync::Once;
use basic::{println, AlienError, AlienResult};
use basic::time::{busy_wait, current_ticks, ticks_to_nanos};
use interface::{
    define_unwind_for_LocalAPICDomain, Basic, LocalAPICDomain, LocalAPICHooks,
};
use raw_cpuid::CpuId;
use x2apic::lapic::{IpiAllShorthand, LocalApic, LocalApicBuilder, TimerDivide, TimerMode};

static mut LOCAL_APIC: MaybeUninit<LocalApic> = MaybeUninit::uninit();
static LOCAL_APIC_READY: AtomicBool = AtomicBool::new(false);
static IS_X2APIC: AtomicBool = AtomicBool::new(false);
static APIC_TIMER_FREQUENCY: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct LocalAPICDomainImpl {
    hooks: Once<LocalAPICHooks>,
}

impl Default for LocalAPICDomainImpl {
    fn default() -> Self {
        Self { hooks: Once::new() }
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

unsafe fn get_local_apic() -> &'static mut LocalApic {
    #[allow(static_mut_refs)]
    unsafe {
        LOCAL_APIC.assume_init_mut()
    }
}

fn local_apic() -> AlienResult<&'static mut LocalApic> {
    if LOCAL_APIC_READY.load(Ordering::Acquire) {
        Ok(unsafe { get_local_apic() })
    } else {
        Err(AlienError::EINVAL)
    }
}

fn raw_apic_id(cpu_id: u8) -> u32 {
    if IS_X2APIC.load(Ordering::Acquire) {
        cpu_id as u32
    } else {
        (cpu_id as u32) << 24
    }
}

fn build_local_apic(xapic_base: usize) -> LocalApic {
    let is_x2apic = cpu_has_x2apic();
    IS_X2APIC.store(is_x2apic, Ordering::Release);

    let mut builder = LocalApicBuilder::new();
    builder
        .spurious_vector(0xf1)
        .timer_vector(0xf0)
        .error_vector(0xf2)
        .timer_mode(TimerMode::OneShot)
        .timer_divide(TimerDivide::Div1)
        .timer_initial(u32::MAX);

    if !is_x2apic {
        builder.set_xapic_base(xapic_base as u64);
    }

    builder
        .build()
        .expect("local_apic domain failed to build LocalApic")
}

fn program_oneshot_timer(local_apic: &mut LocalApic) {
    unsafe {
        local_apic.set_timer_divide(TimerDivide::Div1);
        local_apic.set_timer_mode(TimerMode::OneShot);
        local_apic.enable_timer();
    }
}

/// 读取 Local APIC 错误状态寄存器（ESR）。
/// 在 xAPIC 模式下通过内存映射读取 offset 0x280；在 x2APIC 模式下通过 MSR 0x80B 读取。
fn read_apic_error_status(is_x2apic: bool, xapic_base: usize) -> u32 {
    if is_x2apic {
        // x2APIC 模式：MSR 0x80B
        let mut eax: u32;
        unsafe {
            core::arch::asm!(
                "rdmsr",
                in("ecx") 0x80B_u32,
                out("eax") eax,
                options(preserves_flags)
            );
        }
        eax
    } else {
        // xAPIC 模式：内存映射 offset 0x280
        unsafe {
            let esr_addr = (xapic_base as *const u32).add(0x280 / 4);
            core::ptr::read_volatile(esr_addr)
        }
    }
}

fn calibrate_apic_timer(local_apic: &mut LocalApic) -> u64 {
    // println!("[local_apic] calibrate_apic_timer enter");
    program_oneshot_timer(local_apic);
    unsafe {
        local_apic.set_timer_initial(u32::MAX);
    }

    busy_wait(Duration::from_millis(10));

    let remaining = unsafe { local_apic.timer_current() };
    let elapsed = u32::MAX.saturating_sub(remaining);
    let frequency = ((elapsed as u64) * 100).max(1);
    APIC_TIMER_FREQUENCY.store(frequency, Ordering::SeqCst);

    unsafe {
        local_apic.set_timer_initial(0);
    }

    // println!(
    //     "[local_apic] calibrate_apic_timer ready elapsed={} frequency={}",
    //     elapsed,
    //     frequency
    // );
    frequency
}

impl LocalAPICDomain for LocalAPICDomainImpl {
    fn init(&self, hooks: &LocalAPICHooks) -> AlienResult<()> {
        // println!(
        //     "[local_apic] init enter xapic_base={:#x} ready={} x2apic={}",
        //     hooks.xapic_base,
        //     LOCAL_APIC_READY.load(Ordering::Acquire),
        //     cpu_has_x2apic()
        // );
        self.hooks.call_once(|| *hooks);
        if !LOCAL_APIC_READY.load(Ordering::Acquire) {
            let mut local_apic = build_local_apic(hooks.xapic_base);
            let frequency = calibrate_apic_timer(&mut local_apic);
            program_oneshot_timer(&mut local_apic);
            unsafe {
                #[allow(static_mut_refs)]
                LOCAL_APIC.write(local_apic);
            }
            LOCAL_APIC_READY.store(true, Ordering::Release);
            // println!(
            //     "[local_apic] init ready xapic_base={:#x} frequency={}",
            //     hooks.xapic_base,
            //     frequency
            // );
        }
        // println!("Local APIC domain init");
        Ok(())
    }

    fn set_timer(&self, next_deadline: usize) -> AlienResult<()> {
        let frequency = APIC_TIMER_FREQUENCY.load(Ordering::Acquire);
        let current_tsc = current_ticks();
        let next_deadline = next_deadline as u64;
        let delta_tsc = next_deadline.saturating_sub(current_tsc);
        // println!(
        //     "[local_apic] set_timer enter deadline={:#x} current={:#x} delta={} freq={} ready={}",
        //     next_deadline,
        //     current_tsc,
        //     delta_tsc,
        //     frequency,
        //     LOCAL_APIC_READY.load(Ordering::Acquire)
        // );
        if frequency == 0 {
            // println!("[local_apic] set_timer abort: frequency not calibrated");
            return Err(AlienError::EINVAL);
        }

        let delta_ns = if delta_tsc == 0 {
            1
        } else {
            ticks_to_nanos(delta_tsc)
        };
        let ticks = ((delta_ns as u128 * frequency as u128 / 1_000_000_000) as u32).max(1);
        // println!(
        //     "[local_apic] set_timer program delta_ns={} ticks={}",
        //     delta_ns,
        //     ticks
        // );

        let local_apic = local_apic()?;
        unsafe {
            local_apic.set_timer_divide(TimerDivide::Div1);
            local_apic.set_timer_mode(TimerMode::OneShot);
            local_apic.enable_timer();
            local_apic.set_timer_initial(ticks);
        }
        Ok(())
    }

    fn eoi(&self) -> AlienResult<()> {
        // println!(
        //     "[local_apic] eoi enter ready={} freq={}",
        //     LOCAL_APIC_READY.load(Ordering::Acquire),
        //     APIC_TIMER_FREQUENCY.load(Ordering::Acquire)
        // );
        unsafe { local_apic()?.end_of_interrupt(); }
        Ok(())
    }

    fn send_ipi(&self, target_cpu: usize, vector: u8) -> AlienResult<()> {
        unsafe { local_apic()?.send_ipi(vector, raw_apic_id(target_cpu as u8)); }
        Ok(())
    }

    fn send_ipi_self(&self, vector: u8) -> AlienResult<()> {
        unsafe { local_apic()?.send_ipi_self(vector); }
        Ok(())
    }

    fn send_ipi_all_excluding_self(&self, vector: u8) -> AlienResult<()> {
        unsafe { local_apic()?.send_ipi_all(vector, IpiAllShorthand::AllExcludingSelf); }
        Ok(())
    }

    fn get_error_status(&self) -> AlienResult<u32> {
        let hooks = self.hooks.get().ok_or(AlienError::EINVAL)?;
        let is_x2apic = IS_X2APIC.load(Ordering::Acquire);
        Ok(read_apic_error_status(is_x2apic, hooks.xapic_base))
    }
}

define_unwind_for_LocalAPICDomain!(LocalAPICDomainImpl);

pub fn main() -> Box<dyn LocalAPICDomain> {
    Box::new(UnwindWrap::new(LocalAPICDomainImpl::default()))
}
