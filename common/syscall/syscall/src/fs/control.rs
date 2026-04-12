use alloc::sync::Arc;

use basic::{
    constants::io::{Fcntl64Cmd, OpenFlags, TeletypeCommand},
    AlienError, AlienResult,
};
use interface::{TaskDomain, VfsDomain};
use log::{debug, info};

/// ioctl：`fd` 是目标文件描述符，`request` 是命令号，`argp` 是用户态参数指针。
pub fn sys_ioctl(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    request: usize,
    argp: usize,
) -> AlienResult<isize> {
    let file = task_domain.get_fd(fd)?;
    let _cmd = TeletypeCommand::try_from(request as u32).map_err(|_| AlienError::EINVAL)?;
    info!(
        "<sys_ioctl> fd:{:?} request:{:?} argp:{:?}",
        fd, request, argp
    );
    let res = vfs.vfs_ioctl(file, request as u32, argp);
    info!("<sys_ioctl> res:{:?}", res);
    res.map(|e| e as isize)
}

/// fcntl：`fd` 是目标描述符，`cmd` 是操作号，`arg` 是附加参数。
pub fn sys_fcntl(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    cmd: usize,
    arg: usize,
) -> AlienResult<isize> {
    let raw_cmd = cmd;
    let cmd = Fcntl64Cmd::try_from(cmd as u32).map_err(|_| AlienError::EINVAL)?;
    info!("<sys_fcntl>: {:?} {:?} ", cmd, arg);
    match cmd {
        Fcntl64Cmd::F_DUPFD | Fcntl64Cmd::F_DUPFD_CLOEXEC => {
            let (file, fd) = task_domain.do_fcntl(fd, raw_cmd)?;
            if cmd == Fcntl64Cmd::F_DUPFD_CLOEXEC {
                vfs.do_fcntl(file, raw_cmd, arg)?;
            }
            Ok(fd as isize)
        }
        Fcntl64Cmd::F_GETFD | Fcntl64Cmd::F_SETFD | Fcntl64Cmd::F_GETFL | Fcntl64Cmd::F_SETFL => {
            let file = task_domain.get_fd(fd)?;
            let res = vfs.do_fcntl(file, raw_cmd, arg);
            info!("fcntl:{:?} {:?} res:{:?}", cmd, arg, res);
            res
        }
        Fcntl64Cmd::GETLK | Fcntl64Cmd::SETLK | Fcntl64Cmd::SETLKW => {
            debug!("fcntl: GETLK SETLK SETLKW now ignored");
            Ok(0)
        }
        _ => Err(AlienError::EINVAL),
    }
}

/// dup：复制 `oldfd`，返回新的文件描述符。
pub fn sys_dup(task_domain: &Arc<dyn TaskDomain>, oldfd: usize) -> AlienResult<isize> {
    task_domain.do_dup(oldfd, None)
}

/// dup2：把 `oldfd` 复制到 `newfd`；两者相同则直接返回 `newfd`。
pub fn sys_dup2(
    task_domain: &Arc<dyn TaskDomain>,
    oldfd: usize,
    newfd: usize,
) -> AlienResult<isize> {
    if oldfd == newfd {
        return Ok(newfd as isize);
    }
    let new_fd = task_domain.do_dup(oldfd, Some(newfd));
    info!("<sys_dup2> oldfd: {:?} newfd: {:?} ", oldfd, new_fd);
    new_fd
}

/// dup3：把 `oldfd` 复制到 `newfd`，并处理最小 `O_CLOEXEC` 语义。
pub fn sys_dup3(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    oldfd: usize,
    newfd: usize,
    flags: usize,
) -> AlienResult<isize> {
    const FD_CLOEXEC: usize = 1;

    if oldfd == newfd {
        return Err(AlienError::EINVAL);
    }
    if flags & !(OpenFlags::O_CLOEXEC.bits() as usize) != 0 {
        return Err(AlienError::EINVAL);
    }

    let new_fd = task_domain.do_dup(oldfd, Some(newfd))? as usize;
    if flags & (OpenFlags::O_CLOEXEC.bits() as usize) != 0 {
        let file = task_domain.get_fd(new_fd)?;
        vfs.do_fcntl(file, Fcntl64Cmd::F_SETFD as usize, FD_CLOEXEC)?;
    }
    Ok(new_fd as isize)
}
