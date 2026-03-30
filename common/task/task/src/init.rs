use alloc::{sync::Arc, vec::Vec};

use basic::println;
use spin::Lazy;

use crate::{kthread, processor::add_task, task::Task, vfs_shim::read_all};

pub static INIT_PROCESS: Lazy<Arc<Task>> = Lazy::new(|| {
    let mut data = Vec::new();
    #[cfg(target_arch = "x86_64")]
    let mut init_path = "/bin/sh";
    #[cfg(not(target_arch = "x86_64"))]
    let mut init_path = "/tests/init";

    if !read_all(init_path, &mut data) || data.is_empty() {
        // 启动主程序缺失时回退到 busybox，保证可进入 shell。
        init_path = "/bin/busybox";
        data.clear();
        read_all(init_path, &mut data);
    }
    assert!(!data.is_empty());
    let task = Task::from_elf(init_path, data.as_slice()).unwrap();
    Arc::new(task)
});

/// 将初始进程加入进程池中进行调度
pub fn init_task() {
    kthread::ktread_create(kthread_init, "kthread_test").unwrap();
    let task = INIT_PROCESS.clone();
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
