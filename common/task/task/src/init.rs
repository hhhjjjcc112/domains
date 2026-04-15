use alloc::{sync::Arc, vec::Vec};

use basic::println;
use spin::Lazy;

use crate::kthread;
use crate::{processor::add_task, task::Task, vfs_shim::read_all};

// 统一从 /tests/init 启动，测试程序由用户态 init 自行调度。
const INIT_PROC_PATH: &str = "/tests/init";

pub static INIT_PROCESS: Lazy<Arc<Task>> = Lazy::new(|| {
    let mut data = Vec::new();
    let init_path = INIT_PROC_PATH;

    // 启动入口固定为 /tests/init，避免内核侧按 feature 分叉测试流。
    if !read_all(init_path, &mut data) || data.is_empty() {
        panic!("init process binary is empty or unreadable: {}", init_path);
    }

    let task =
        Task::from_elf(init_path, data.as_slice()).unwrap_or_else(|| panic!("Task::from_elf failed for {}", init_path));
    println!("INIT_PROCESS ready: path={}, tid={}", init_path, task.tid());
    Arc::new(task)
});

/// 将初始进程加入进程池中进行调度
pub fn init_task() {
    kthread::ktread_create(kthread_init, "kthread_test").unwrap();
    let task = INIT_PROCESS.clone();
    println!("init_task enqueue init process tid={}", task.tid());
    add_task(task);
}

fn kthread_init() {
    println!("kthread_init start...");
    let mut time = basic::time::read_time_ms();
    loop {
        let now = basic::time::read_time_ms();
        if now - time > 1000 {
            // println!("kthread_init tick at {}", now);
            time = now;
        }
        basic::yield_now().unwrap();
    }
    // kthread::ktrhead_exit();
}
