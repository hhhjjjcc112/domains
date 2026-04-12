use alloc::sync::Arc;

use basic::{
    constants::{ipc::FutexOp, time::TimeSpec},
    sync::Mutex,
    time::{TimeNow, ToClock},
    AlienError, AlienResult,
};
use memory_addr::VirtAddr;
use ptable::VmIo;

use crate::{
    futex::{FutexWaitManager, FutexWaiter},
    processor::current_task,
};

pub static FUTEX_WAITER: Mutex<FutexWaitManager> = Mutex::new(FutexWaitManager::new());

/// See https://man7.org/linux/man-pages/man2/futex.2.html
pub fn futex(
    uaddr: usize,
    futex_op: u32,
    val: u32,
    val2: usize,
    uaddr2: usize,
    val3: u32,
) -> AlienResult<isize> {
    let futex_op = FutexOp::try_from(futex_op).map_err(|_| AlienError::EINVAL)?;
    let task = current_task().unwrap();
    let mut futex_waiter = FUTEX_WAITER.lock();
    // let tid = task.tid();
    // println_color!(
    //     31,
    //     "futex: [{}] {:#x?} {:?} {:?} {:#x} {:#x} {:?}",
    //     tid,
    //     uaddr,
    //     futex_op,
    //     val,
    //     val2,
    //     uaddr2,
    //     val3
    // );
    macro_rules! wait {
        ($wait_time:expr,$bitset:expr) => {
            warn!("Futex wait time: {:?}", $wait_time);
            let timeout_flag = Arc::new(Mutex::new(false));
            let tid = task.tid();
            let waiter = FutexWaiter::new(tid, $wait_time, timeout_flag.clone(), $bitset);
            futex_waiter.add_waiter(uaddr, waiter);
            drop(futex_waiter);
            // switch to other task
            basic::wait_now()?;
            warn!("Because of futex, we switch to other task");
            // checkout the timeout flag
            let timeout_flag = timeout_flag.lock();
            if *timeout_flag {
                return Ok(0);
            }
        };
    }
    match futex_op {
        FutexOp::FutexWaitPrivate | FutexOp::FutexWait => {
            let u_value = task
                .address_space
                .lock()
                .read_value_atomic(VirtAddr::from(uaddr))
                .unwrap();
            if u_value != val as usize {
                return Err(AlienError::EAGAIN);
            }
            let wait_time = if val2 != 0 {
                let time_spec = task.read_val_from_user::<TimeSpec>(VirtAddr::from(val2))?;
                Some(time_spec.to_clock() + TimeSpec::now().to_clock())
            } else {
                // wait forever
                None
            };
            wait!(wait_time, u32::MAX);
        }
        FutexOp::FutexCmpRequeuePiPrivate => {
            let u_value = task
                .address_space
                .lock()
                .read_value_atomic(VirtAddr::from(uaddr))
                .unwrap();
            if u_value != val3 as usize {
                return Err(AlienError::EAGAIN);
            }
            // wake val tasks
            let res = futex_waiter.wake(uaddr, val as usize, u32::MAX)?;
            // requeue val2 tasks to uaddr2
            let res2 = futex_waiter.requeue(uaddr2, val2, uaddr)?;
            return Ok(res2 as isize + res as isize);
        }
        FutexOp::FutexRequeuePrivate => {
            // wake val tasks
            let res = futex_waiter.wake(uaddr, val as usize, u32::MAX)?;
            // requeue val2 tasks to uaddr2
            let res2 = futex_waiter.requeue(uaddr2, val2, uaddr)?;
            return Ok(res2 as isize + res as isize);
        }
        FutexOp::FutexWakePrivate | FutexOp::FutexWake => {
            let res = futex_waiter.wake(uaddr, val as usize, u32::MAX)?;
            return Ok(res as isize);
        }
        FutexOp::FutexWaitBitsetPrivate => {
            let bitset = val3;
            let u_value = task
                .address_space
                .lock()
                .read_value_atomic(VirtAddr::from(uaddr))
                .unwrap();
            if u_value != val as usize {
                return Err(AlienError::EAGAIN);
            }
            let wait_time = if val2 != 0 {
                let time_spec = task.read_val_from_user::<TimeSpec>(VirtAddr::from(val2))?;
                Some(time_spec.to_clock())
            } else {
                None
            };
            wait!(wait_time, bitset);
        }
        FutexOp::FutexWakeBitsetPrivate => {
            let bitset = val3;
            let res = futex_waiter.wake(uaddr, val as usize, bitset)?;
            return Ok(res as isize);
        }
        _ => {
            return Err(AlienError::ENOSYS);
        }
    }
    Ok(0)
}
