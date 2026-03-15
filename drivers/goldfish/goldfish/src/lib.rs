#![no_std]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "riscv64"))]
compile_error!("goldfish domain 仅支持 riscv64");
extern crate alloc;

use alloc::{boxed::Box, format, string::String};
use core::ops::Range;

use basic::{
    constants::io::RtcTime,
    io::SafeIORegion,
    println,
    sync::{Once, OnceGet},
    AlienResult,
};
use interface::{define_unwind_for_RtcDomain, Basic, DeviceBase, RtcDomain};
use rtc::{goldfish::GoldFishRtc, LowRtcDevice, RtcIORegion};
use shared_heap::DBox;
use timestamp::DateTime;

#[derive(Debug, Default)]
struct Rtc {
    rtc: Once<GoldFishRtc>,
}

#[derive(Debug)]
pub struct SafeIORegionWrapper(SafeIORegion);

impl RtcIORegion for SafeIORegionWrapper {
    fn read_at(&self, offset: usize) -> u32 {
        self.0.read_at(offset).unwrap()
    }

    fn write_at(&self, offset: usize, value: u32) {
        self.0.write_at(offset, value).unwrap()
    }
}

impl Basic for Rtc {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl DeviceBase for Rtc {
    fn handle_irq(&self) -> AlienResult<()> {
        unimplemented!()
    }
}

impl Rtc {
    fn time(&self) -> String {
        let time_stamp_nanos = self.rtc.get_must().read_time();
        const NANOS_PER_SEC: usize = 1_000_000_000;
        let date = DateTime::new(time_stamp_nanos as usize / NANOS_PER_SEC);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            date.year, date.month, date.day, date.hour, date.minutes, date.seconds
        )
    }
}

impl RtcDomain for Rtc {
    fn init(&self, address_range: &Range<usize>) -> AlienResult<()> {
        println!("Rtc region: {:#x?}", address_range);
        let safe_region = SafeIORegion::from(address_range.clone());
        let rtc = GoldFishRtc::new(Box::new(SafeIORegionWrapper(safe_region)));
        self.rtc.call_once(|| rtc);
        println!("current time: {}", self.time());
        Ok(())
    }

    fn read_time(&self, mut time: DBox<RtcTime>) -> AlienResult<DBox<RtcTime>> {
        let time_stamp_nanos = self.rtc.get_must().read_time();
        const NANOS_PER_SEC: usize = 1_000_000_000;
        let date = DateTime::new(time_stamp_nanos as usize / NANOS_PER_SEC);
        let t = RtcTime {
            year: date.year as u32,
            mon: date.month as u32,
            mday: date.day as u32,
            hour: date.hour as u32,
            min: date.minutes as u32,
            sec: date.seconds as u32,
            ..Default::default()
        };
        *time = t;
        Ok(time)
    }
}
define_unwind_for_RtcDomain!(Rtc);

pub fn main() -> Box<dyn RtcDomain> {
    Box::new(UnwindWrap::new(Rtc::default()))
}
