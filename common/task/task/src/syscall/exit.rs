use alloc::sync::Arc;

use basic::{constants::ipc::FutexOp, println, AlienResult};
use memory_addr::VirtAddr;
use ptable::VmIo;
use task_meta::TaskStatus;

use crate::{
    init::INIT_PROCESS,
    processor::{current_task, remove_task},
};

pub fn do_exit(exit_code: i32) -> AlienResult<isize> {
    let task = current_task().unwrap();
    let exit_code = (exit_code & 0xff) << 8;
    if task.pid() == 1 {
        println!("Init process exit with code {}", exit_code);
        panic!("Init process exit");
    }
    {
        let init = INIT_PROCESS.clone();
        task.inner().children.iter().for_each(|(tid, child)| {
            child.inner().parent = Some(Arc::downgrade(&init));
            init.inner().children.insert(*tid, child.clone());
        });
        task.inner().children.clear();
    }
    task.inner().status = TaskStatus::Zombie;
    task.inner().exit_code = exit_code;
    // global_logoff_signals(task.get_tid() as usize);

    let clear_child_tid = task.inner().clear_child_tid;
    // let tid = task.tid();
    // println_color!(31,"[{}] exit wake futex on {:#x}",tid, clear_child_tid);
    if clear_child_tid != 0 {
        task.address_space
            .lock()
            .write_val(VirtAddr::from(clear_child_tid), &0u32)
            .unwrap();
        let _ = super::futex::futex(clear_child_tid, FutexOp::FutexWake as u32, 1, 0, 0, 0);
    } else {
        info!("exit clear_child_tid is 0");
    }
    if task.send_sigchld_when_exit || task.pid() == task.tid() {
        //send_signal(parent.pid, SignalNumber::SIGCHLD as usize);
    }
    remove_task(task.tid()); // remove task from global task manager
    task.inner().status = TaskStatus::Terminated;
    drop(task);
    basic::exit_now()?;
    Ok(0)
}
