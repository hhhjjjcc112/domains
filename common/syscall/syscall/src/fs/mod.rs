mod basic;
mod control;
mod poll;

use alloc::sync::Arc;

pub use basic::*;
use ::basic::{constants::AT_FDCWD, AlienResult};
pub use control::*;
use interface::{InodeID, TaskDomain, VFS_ROOT_ID};
use log::info;
pub use poll::*;

/// 根据路径和基准 fd 解析 VFS 位置；`path` 是用户态路径，`fd` 决定相对路径的起点。
fn user_path_at(
    task_domain: &Arc<dyn TaskDomain>,
    fd: isize,
    path: &str,
) -> AlienResult<(InodeID, InodeID)> {
    info!("user_path_at fd: {}, path:{}", fd, path);
    let res = if !path.starts_with('/') {
        if fd == AT_FDCWD {
            let fs_context = task_domain.fs_info()?;
            (VFS_ROOT_ID, fs_context.1)
        } else {
            let fd = fd as usize;
            let file = task_domain.get_fd(fd)?;
            (VFS_ROOT_ID, file)
        }
    } else {
        (VFS_ROOT_ID, VFS_ROOT_ID)
    };
    Ok(res)
}
