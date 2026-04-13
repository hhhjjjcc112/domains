#![no_std]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "x86_64"))]
compile_error!("cmos_rtc domain 仅支持 x86_64");

extern crate alloc;

use alloc::boxed::Box;
use core::ops::Range;

use basic::{
    AlienResult,
    constants::io::RtcTime,
    println,
};
use interface::{Basic, DeviceBase, RtcDomain, define_unwind_for_RtcDomain};
use shared_heap::DBox;
use timestamp::DateTime;
use x86_rtc::Rtc;

#[derive(Debug, Default)]
struct CmosRtc;

impl Basic for CmosRtc {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl DeviceBase for CmosRtc {
    fn handle_irq(&self) -> AlienResult<()> {
        // 当前阶段不实现 RTC 中断，最小路径仅支持读时间。
        Ok(())
    }
}

impl RtcDomain for CmosRtc {
    fn init(&self, address_range: &Range<usize>) -> AlienResult<()> {
        // x86 CMOS RTC 是固定端口设备，地址仅用于日志与兼容接口。
        println!("CmosRtc region: {:#x?}", address_range);
        Ok(())
    }

    fn read_time(&self, mut time: DBox<RtcTime>) -> AlienResult<DBox<RtcTime>> {
        let unix_secs = Rtc::new().get_unix_timestamp() as usize;
        let date = DateTime::new(unix_secs);
        *time = RtcTime {
            year: date.year as u32,
            mon: date.month as u32,
            mday: date.day as u32,
            hour: date.hour as u32,
            min: date.minutes as u32,
            sec: date.seconds as u32,
            ..Default::default()
        };
        Ok(time)
    }
}

define_unwind_for_RtcDomain!(CmosRtc);

pub fn main() -> Box<dyn RtcDomain> {
    Box::new(UnwindWrap::new(CmosRtc))
}
