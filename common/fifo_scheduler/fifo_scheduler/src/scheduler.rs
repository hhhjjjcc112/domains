use alloc::{collections::VecDeque, sync::Arc};

use basic::{arch::cpu_id, sync::Mutex};
use common_scheduler::Scheduler;
use shared_heap::DBox;
use storage::CustomStorge;
use task_meta::TaskSchedulingInfo;

type __TaskList = Mutex<VecDeque<DBox<TaskSchedulingInfo>, CustomStorge>>;
type TaskList = Arc<__TaskList, CustomStorge>;
#[derive(Debug)]
pub struct CustomFiFoScheduler {
    tasks: TaskList,
}

impl CustomFiFoScheduler {
    pub fn new() -> Self {
        let task_list = storage::get_or_insert::<__TaskList, _>("tasks", || {
            __TaskList::new(VecDeque::new_in(CustomStorge))
        });
        Self { tasks: task_list }
    }
}

impl Scheduler for CustomFiFoScheduler {
    fn add_task(&self, task_meta: DBox<TaskSchedulingInfo>) {
        self.tasks.lock().push_back(task_meta);
    }
    fn fetch_task(&self) -> Option<DBox<TaskSchedulingInfo>> {
        let cpu_id = cpu_id();
        let mut tasks = self.tasks.lock();
        let res = tasks
            .iter()
            .position(|info| info.cpus_allowed & (1 << cpu_id) != 0);
        // println!("fetch_task: {:?}, len: {}", res, self.tasks.len());
        if let Some(index) = res {
            return tasks.remove(index);
        }
        None
    }
    fn name(&self) -> &'static str {
        "FiFoScheduler"
    }
}
