use alloc::sync::Arc;

use basic::{
    config::CLOCK_FREQ,
    constants::time::{ClockId, TimeSpec, TimeVal},
    time::{read_timer, TimeNow, ToClock},
    AlienError, AlienResult,
};
use interface::TaskDomain;
use pod::Pod;

/// clock_gettime：`clk_id` 是时钟类型，`tp` 是用户态输出缓冲区。
pub fn sys_clock_gettime(
    task_domain: &Arc<dyn TaskDomain>,
    clk_id: usize,
    tp: usize,
) -> AlienResult<isize> {
    let id = ClockId::try_from(clk_id).map_err(|_| AlienError::EINVAL)?;
    match id {
        ClockId::Monotonic | ClockId::Realtime | ClockId::ProcessCputimeId => {
            let time = read_timer();
            let time = TimeSpec {
                tv_sec: time / CLOCK_FREQ,
                tv_nsec: (time % CLOCK_FREQ) * 1000_000_000 / CLOCK_FREQ,
            };
            task_domain.copy_to_user(tp, time.as_bytes())?;
            Ok(0)
        }
        _ => {
            panic!("clock_get_time: clock_id {:?} not supported", id);
        }
    }
}

/// gettimeofday：`tv` 是用户态时间结构体指针，`tz` 是历史时区参数，当前忽略。
pub fn sys_get_time_of_day(
    task_domain: &Arc<dyn TaskDomain>,
    tv: usize,
    _tz: usize,
) -> AlienResult<isize> {
    if tv != 0 {
        let time = TimeVal::now();
        task_domain.write_val_to_user(tv, &time)?;
    }
    Ok(0)
}

/// nanosleep：`req` 是请求睡眠时间，`rem` 是剩余时间回写位置。
pub fn sys_nanosleep(
    task_domain: &Arc<dyn TaskDomain>,
    req: usize,
    rem: usize,
) -> AlienResult<isize> {
    if req == 0 {
        return Err(AlienError::EFAULT);
    }
    let req_ts = task_domain.read_val_from_user::<TimeSpec>(req)?;
    if req_ts.tv_nsec >= 1_000_000_000 {
        return Err(AlienError::EINVAL);
    }
    let deadline = TimeSpec::now().to_clock().saturating_add(req_ts.to_clock());
    loop {
        if TimeSpec::now().to_clock() >= deadline {
            break;
        }
        basic::yield_now()?;
    }
    if rem != 0 {
        let remain = TimeSpec::new(0, 0);
        task_domain.write_val_to_user(rem, &remain)?;
    }
    Ok(0)
}
