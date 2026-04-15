#![no_std]
#![forbid(unsafe_code)]

mod scheduler;

extern crate alloc;

use alloc::boxed::Box;

use basic::AlienResult;
use interface::{define_unwind_for_SchedulerDomain, Basic, SchedulerDomain};
use shared_heap::DBox;
pub use scheduler::Scheduler;
use task_meta::TaskSchedulingInfo;

#[derive(Debug)]
pub struct CommonSchedulerDomain;

impl CommonSchedulerDomain {
    pub fn new(global_scheduler: Box<dyn Scheduler>) -> Self {
        scheduler::set_scheduler(global_scheduler);
        Self
    }
}

impl Basic for CommonSchedulerDomain {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl SchedulerDomain for CommonSchedulerDomain {
    fn init(&self) -> AlienResult<()> {
        // println!("SchedulerDomain init, name: {}", self.name);
        Ok(())
    }

    fn add_task(&self, scheduling_info: DBox<TaskSchedulingInfo>) -> AlienResult<()> {
        scheduler::add_task(scheduling_info);
        Ok(())
    }

    fn fetch_task(&self, info: DBox<TaskSchedulingInfo>) -> AlienResult<DBox<TaskSchedulingInfo>> {
        Ok(scheduler::fetch_task(info))
    }
}

define_unwind_for_SchedulerDomain!(CommonSchedulerDomain);
