use alloc::{sync::Arc, vec::Vec};

use basic::{constants::task::WaitOptions, println, AlienError, AlienResult};
use memory_addr::VirtAddr;
use task_meta::TaskStatus;

use crate::{processor::current_task, task::Task};

pub fn do_wait4(
    pid: isize,
    exit_code_ptr: usize,
    options: u32,
    _rusage: usize,
) -> AlienResult<isize> {
    let wait_options = WaitOptions::from_bits(options).ok_or(AlienError::EINVAL)?;
    loop {
        let task = current_task().unwrap();
        let wait_task = filter_exit_task(&task, pid)?;
        if let Some(wait_task) = wait_task {
            let tid = wait_task.tid();
            let pid = wait_task.pid();
            let status = wait_task.status();
            let is_task_exit = basic::is_task_exit(tid).unwrap();
            if status == TaskStatus::Terminated && is_task_exit {
                let exit_code = wait_task.exit_code();
                if exit_code_ptr != 0 {
                    // println_color!(31, "write exit code {} to pid {}", exit_code, pid);
                    task.write_val_to_user(VirtAddr::from(exit_code_ptr), &exit_code)?;
                }
                if wait_options.contains(WaitOptions::WNOWAIT) {
                    // recycle the task later
                    assert_eq!(pid, tid);
                } else {
                    // recycle the task now
                    task.inner().children.remove(&pid);
                    basic::remove_task(tid).expect("remove task failed");
                    println!("release task [{}-{}]", pid, tid);
                    assert_eq!(
                        Arc::strong_count(&wait_task),
                        1,
                        "Father is [{}-{}], wait task is [{}-{}]",
                        task.pid(),
                        task.tid(),
                        pid,
                        tid,
                    );
                }
                return Ok(pid as isize);
            }
        }
        if wait_options.contains(WaitOptions::WNOHANG) {
            return Ok(0);
        } else {
            basic::yield_now().unwrap();
        }
    }
}

fn filter_exit_task(task: &Arc<Task>, pid: isize) -> AlienResult<Option<Arc<Task>>> {
    let res = task
        .inner()
        .children
        .values()
        .filter(|child| child.pid() == pid as usize || pid == -1)
        .cloned()
        .collect::<Vec<_>>();
    if res.is_empty() {
        return Err(AlienError::ECHILD);
    }
    let term_task = res
        .iter()
        .find(|task| task.status() == TaskStatus::Terminated);
    Ok(term_task.cloned())
}
