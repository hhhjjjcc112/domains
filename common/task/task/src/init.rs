use alloc::{sync::Arc, vec::Vec};

use basic::println;
use spin::Lazy;

use crate::{kthread, processor::add_task, task::Task, vfs_shim::read_all};

pub static INIT_PROCESS: Lazy<Arc<Task>> = Lazy::new(|| {
    let mut data = Vec::new();
    let mut init_path = "/tests/init";
    let init_read_ok = read_all(init_path, &mut data);

    if !init_read_ok || data.is_empty() {
        if !init_read_ok {
            println!("INIT_PROCESS fallback: read {} failed", init_path);
        } else {
            println!("INIT_PROCESS fallback: {} is empty", init_path);
        }
        // 启动主程序缺失时回退到测试总入口，保证能继续验证 syscall。
        init_path = "/tests/new/syscall_all";
        data.clear();
        if !read_all(init_path, &mut data) || data.is_empty() {
            println!("failed to read fallback init program: {}", init_path);
        } else {
            println!("INIT_PROCESS fallback hit: switched to {}", init_path);
        }
    }

    if data.is_empty() {
        panic!("init process binary is empty");
    }

    let task = match Task::from_elf(init_path, data.as_slice()) {
        Some(task) => task,
        None if init_path != "/tests/new/syscall_all" => {
            println!("Task::from_elf failed for /tests/init, fallback to /tests/new/syscall_all");
            data.clear();
            if !read_all("/tests/new/syscall_all", &mut data) || data.is_empty() {
                panic!("fallback /tests/new/syscall_all read failed");
            }
            println!("INIT_PROCESS fallback hit: switched to /tests/new/syscall_all after elf load failure");
            Task::from_elf("/tests/new/syscall_all", data.as_slice())
                .unwrap_or_else(|| panic!("Task::from_elf failed for fallback /tests/new/syscall_all"))
        }
        None => {
            panic!("Task::from_elf failed for {}", init_path);
        }
    };
    println!("INIT_PROCESS ready: path={}, tid={}", init_path, task.tid());
    Arc::new(task)
});

/// 将初始进程加入进程池中进行调度
pub fn init_task() {
    #[cfg(target_arch = "riscv64")]
    kthread::ktread_create(kthread_init, "kthread_test").unwrap();
    #[cfg(target_arch = "x86_64")]
    println!("x86_64 skip kthread_test for minimal shell bring-up");
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
