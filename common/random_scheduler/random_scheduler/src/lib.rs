#![feature(allocator_api)]
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
use alloc::{boxed::Box, collections::VecDeque, sync::Arc};
use core::sync::atomic::AtomicBool;

use basic::{arch::cpu_id, println, sync::Mutex};
use common_scheduler::{CommonSchedulerDomain, Scheduler, UnwindWrap};
use interface::SchedulerDomain;
use shared_heap::DBox;
use storage::CustomStorge;
use task_meta::TaskSchedulingInfo;

type __TaskList = Mutex<VecDeque<DBox<TaskSchedulingInfo>, CustomStorge>>;
type TaskList = Arc<__TaskList, CustomStorge>;
#[derive(Debug)]
pub struct RandomScheduler {
    tasks: TaskList,
}

impl Default for RandomScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl RandomScheduler {
    pub fn new() -> Self {
        println!("RandomScheduler: new");
        let task_list = storage::get::<__TaskList>("tasks").unwrap();
        let len = task_list.lock().len();
        task_list.lock().reserve(20);
        println!("RandomScheduler: The task list len is {}", len);
        Self { tasks: task_list }
    }
}

impl Scheduler for RandomScheduler {
    fn add_task(&self, task_meta: DBox<TaskSchedulingInfo>) {
        self.tasks.lock().push_back(task_meta);
    }

    fn fetch_task(&self) -> Option<DBox<TaskSchedulingInfo>> {
        let cpu_id = cpu_id();
        let mut tasks = self.tasks.lock();
        let mut max_nice = i8::MAX;
        let mut res = None;
        // find the task with the highest priority, it's nice is the smallest
        for (idx, info) in tasks.iter().enumerate() {
            if info.cpus_allowed & (1 << cpu_id) != 0 && info.nice < max_nice {
                max_nice = info.nice;
                res = Some(idx);
            }
        }
        static FETCH_MASK: AtomicBool = AtomicBool::new(false);
        if !FETCH_MASK.swap(true, core::sync::atomic::Ordering::Relaxed) {
            println!("fetch_task: {:?}, len: {}", res, tasks.len());
        }
        if let Some(index) = res {
            return tasks.remove(index);
        }
        None
    }

    fn name(&self) -> &'static str {
        "RandomScheduler"
    }
}

pub fn main() -> Box<dyn SchedulerDomain> {
    Box::new(UnwindWrap::new(CommonSchedulerDomain::new(Box::new(
        RandomScheduler::new(),
    ))))
}
