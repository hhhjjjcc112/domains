use basic::{
    constants::{PriorityTarget, Which},
    AlienResult,
};

use crate::processor::current_task;

fn parse_priority_target(which: i32, who: u32) -> AlienResult<PriorityTarget> {
    let which = Which::try_from(which)?;
    Ok(match which {
        Which::PRIO_PROCESS => {
            let pid = if who == 0 {
                let task = current_task().unwrap();
                task.pid() as u32
            } else {
                who
            };
            PriorityTarget::Process(pid)
        }
        Which::PRIO_PGRP => {
            return Err(basic::AlienError::ENOSYS);
        }
        Which::PRIO_USER => {
            return Err(basic::AlienError::ENOSYS);
        }
    })
}

pub fn do_set_priority(which: i32, who: u32, prio: i32) -> AlienResult<()> {
    let target = parse_priority_target(which, who)?;
    let new_nice = prio.clamp(-20, 19) as i8;
    match target {
        PriorityTarget::Process(pid) => {
            let current = current_task().unwrap();
            assert_eq!(current.tid(), pid as _);
            basic::set_task_priority(new_nice)?;
            Ok(())
        }
        _ => {
            Err(basic::AlienError::ENOSYS)
        }
    }
}

pub fn do_get_priority(which: i32, who: u32) -> AlienResult<i32> {
    let target = parse_priority_target(which, who)?;
    match target {
        PriorityTarget::Process(pid) => {
            let current = current_task().unwrap();
            assert_eq!(current.tid(), pid as _);
            let prio = basic::get_task_priority();
            prio.map(|p| 20 - p as i32)
        }
        _ => {
            Err(basic::AlienError::ENOSYS)
        }
    }
}
