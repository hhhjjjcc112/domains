use alloc::{collections::BTreeMap, sync::Arc};

use basic::{println, sync::Mutex, wake_up_wait_task};

use crate::task::Task;

pub fn current_task() -> Option<Arc<Task>> {
    let tid = match basic::current_tid() {
        Ok(Some(tid)) => tid,
        Ok(None) => {
            println!("task::current_task: no current tid");
            return None;
        }
        Err(err) => {
            println!("task::current_task: current_tid error: {:?}", err);
            return None;
        }
    };
    let task = GLOBAL_TASK_MANAGER.lock().get(&tid).map(Arc::clone);
    if task.is_none() {
        println!("task::current_task: tid {} not found", tid);
    }
    task
}

static GLOBAL_TASK_MANAGER: Mutex<BTreeMap<usize, Arc<Task>>> = Mutex::new(BTreeMap::new());

pub fn add_task(task: Arc<Task>) {
    let tid = task.tid();
    GLOBAL_TASK_MANAGER.lock().insert(tid, task);
    wake_up_wait_task(tid).unwrap();
}

pub fn remove_task(tid: usize) {
    GLOBAL_TASK_MANAGER.lock().remove(&tid);
}

#[allow(dead_code)]
pub fn find_task(tid: usize) -> Option<Arc<Task>> {
    GLOBAL_TASK_MANAGER.lock().get(&tid).map(Arc::clone)
}

pub fn find_task_by_pid(pid: usize) -> Option<Arc<Task>> {
    GLOBAL_TASK_MANAGER
        .lock()
        .values()
        .find(|task| task.pid() == pid)
        .cloned()
}
