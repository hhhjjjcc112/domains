use basic::{AlienError, AlienResult, constants::*};

use super::{
    super::{fs::*, mm::*, signal::*, socket::*, system::*, task::*, time::*},
    SysCallDomainImpl,
};

pub(super) fn dispatch(
    domain: &SysCallDomainImpl,
    syscall_id: usize,
    args: [usize; 6],
) -> Option<AlienResult<isize>> {
    let task = &domain.task_domain;
    let vfs = &domain.vfs_domain;
    macro_rules! net {
        () => {
            match domain.net_stack_domain() {
                Ok(net) => net,
                Err(err) => return Some(Err(err)),
            }
        };
    }

    let result = match syscall_id {
        // 公共 Linux fd/路径类 syscall。
        SYSCALL_EVENTFD2 => sys_eventfd2(vfs, task, args[0], args[1]),
        SYSCALL_EPOLL_CREATE1 => sys_poll_createl(vfs, task, args[0]),
        SYSCALL_EPOLL_CTL => sys_poll_ctl(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_GETCWD => sys_getcwd(vfs, task, args[0], args[1]),
        SYSCALL_DUP => sys_dup(task, args[0]),
        SYSCALL_DUP3 => sys_dup3(vfs, task, args[0], args[1], args[2]),
        SYSCALL_PREAD64 => sys_pread64(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_PWRITE64 => sys_pwrite64(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_FCNTL => sys_fcntl(vfs, task, args[0], args[1], args[2]),
        SYSCALL_IOCTL => sys_ioctl(vfs, task, args[0], args[1], args[2]),
        SYSCALL_MKDIRAT => sys_mkdirat(vfs, task, args[0], args[1], args[2]),
        SYSCALL_MOUNT => sys_mount(vfs, task, args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_UNLINKAT => sys_unlinkat(vfs, task, args[0], args[1], args[2]),
        SYSCALL_LINKAT => sys_linkat(vfs, task, args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_SYMLINKAT => sys_symlinkat(vfs, task, args[0], args[1], args[2]),
        SYSCALL_READLINKAT => sys_readlinkat(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_RENAMEAT2 => sys_renameat2(vfs, task, args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_FTRUNCATE => sys_ftruncate(vfs, task, args[0], args[1]),
        SYSCALL_TRUNCATE => sys_truncate(vfs, task, args[0], args[1]),
        SYSCALL_STATFS => sys_statfs(vfs, task, args[0], args[1]),
        SYSCALL_FSTATFS => sys_fstatfs(vfs, task, args[0], args[1]),
        SYSCALL_FACCESSAT => sys_faccessat(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_CHDIR => sys_chdir(vfs, task, args[0]),
        SYSCALL_OPENAT => sys_openat(vfs, task, args[0], args[1] as *const u8, args[2], args[3]),
        SYSCALL_CLOSE => sys_close(vfs, task, args[0]),
        SYSCALL_PIPE2 => sys_pipe2(task, vfs, args[0], args[1]),
        SYSCALL_GETDENTS64 => sys_getdents64(vfs, task, args[0], args[1], args[2]),
        SYSCALL_LSEEK => sys_lseek(vfs, task, args[0], args[1], args[2]),
        SYSCALL_READ => sys_read(vfs, task, args[0], args[1], args[2]),
        SYSCALL_WRITE => sys_write(vfs, task, args[0], args[1] as *const u8, args[2]),
        SYSCALL_READV => sys_readv(vfs, task, args[0], args[1], args[2]),
        SYSCALL_WRITEV => sys_writev(vfs, task, args[0], args[1], args[2]),
        SYSCALL_SENDFILE => sys_sendfile(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_PSELECT6 => sys_pselect6(vfs, task, SelectArgs { nfds: args[0], readfds: args[1], writefds: args[2], exceptfds: args[3], timeout: args[4], sigmask: args[5] }),
        SYSCALL_PPOLL => sys_ppoll(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_FCHDIR => sys_fchdir(vfs, task, args[0]),
        SYSCALL_GETRLIMIT => sys_getrlimit(task, args[0], args[1]),
        SYSCALL_SETRLIMIT => sys_setrlimit(task, args[0], args[1]),
        SYSCALL_GETRUSAGE => sys_getrusage(task, args[0], args[1]),
        SYSCALL_NEWFSTATAT => sys_fstatat(vfs, task, args[0], args[1] as *const u8, args[2], args[3]),
        SYSCALL_FSTAT => sys_fstat(vfs, task, args[0], args[1]),
        SYSCALL_FSYNC => sys_fsync(vfs, task, args[0]),
        SYSCALL_UTIMENSAT => sys_utimensat(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_SETXATTR => sys_setxattr(vfs, task, args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_LSETXATTR => sys_lsetxattr(vfs, task, args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_FSETXATTR => sys_fsetxattr(vfs, task, args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_GETXATTR => sys_getxattr(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_LGETXATTR => sys_lgetxattr(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_FGETXATTR => sys_fgetxattr(vfs, task, args[0], args[1], args[2], args[3]),
        SYSCALL_LISTXATTR => sys_listxattr(vfs, task, args[0], args[1], args[2]),
        SYSCALL_LLISTXATTR => sys_llistxattr(vfs, task, args[0], args[1], args[2]),
        SYSCALL_FLISTXATTR => sys_flistxattr(vfs, task, args[0], args[1], args[2]),
        SYSCALL_REMOVEXATTR => sys_removexattr(vfs, task, args[0], args[1]),
        SYSCALL_LREMOVEXATTR => sys_lremovexattr(vfs, task, args[0], args[1]),
        SYSCALL_FREMOVEXATTR => sys_fremovexattr(vfs, task, args[0], args[1]),
        // 公共 Linux 进程、时间与信号 syscall。
        SYSCALL_EXIT => sys_exit(task, args[0]),
        SYSCALL_EXIT_GROUP => sys_exit_group(task, args[0]),
        SYSCALL_SET_TID_ADDRESS => sys_set_tid_address(task, args[0]),
        SYSCALL_CLOCK_GETTIME => sys_clock_gettime(task, args[0], args[1]),
        SYSCALL_NANOSLEEP => sys_nanosleep(task, args[0], args[1]),
        SYSCALL_SCHED_YIELD => sys_yield(),
        SYSCALL_FUTEX => sys_futex(task, args[0], args[1], args[2], args[3], args[4], args[5]),
        SYSCALL_SIGALTSTACK => sys_sigaltstack(task, args[0], args[1]),
        SYSCALL_RT_SIGACTION => sys_sigaction(task, args[0], args[1], args[2], args[3]),
        SYSCALL_RT_SIGPROCMASK => sys_sigprocmask(task, args[0], args[1], args[2], args[3]),
        SYSCALL_SETPRIORITY => sys_set_priority(task, args[0], args[1], args[2]),
        SYSCALL_GETPRIORITY => sys_get_priority(task, args[0], args[1]),
        SYSCALL_SETPGID => sys_set_pgid(task, args[0], args[1]),
        SYSCALL_GETPGID => sys_get_pgid(task, args[0]),
        SYSCALL_SETSID => sys_set_sid(task),
        SYSCALL_GETSID => sys_get_sid(task, args[0]),
        SYSCALL_UNAME => sys_uname(task, args[0]),
        SYSCALL_GETTIMEOFDAY => sys_get_time_of_day(task, args[0], args[1]),
        SYSCALL_GETPID => sys_get_pid(task),
        SYSCALL_GETPPID => sys_get_ppid(task),
        SYSCALL_GETUID => sys_getuid(task),
        SYSCALL_GETEUID => sys_get_euid(task),
        SYSCALL_GETGID => sys_get_gid(task),
        SYSCALL_GETEGID => sys_get_egid(task),
        SYSCALL_GETTID => sys_get_tid(),
        SYSCALL_CLONE3 => Err(AlienError::ENOSYS),
        SYSCALL_EXECVE => sys_execve(task, args[0], args[1], args[2]),
        SYSCALL_WAIT4 => sys_wait4(task, args[0], args[1], args[2], args[3]),
        // 公共 Linux socket syscall，fd 仍由 VFS 持有，因此同时依赖 net/vfs/task。
        SYSCALL_SOCKET => {
            let net = net!();
            sys_socket(task, vfs, &net, args[0], args[1], args[2])
        }
        SYSCALL_SOCKETPAIR => {
            let net = net!();
            sys_socket_pair(task, vfs, &net, args[0], args[1], args[2], args[3])
        }
        SYSCALL_BIND => {
            let net = net!();
            sys_bind(task, vfs, &net, args[0], args[1], args[2])
        }
        SYSCALL_LISTEN => {
            let net = net!();
            sys_listen(task, vfs, &net, args[0], args[1])
        }
        SYSCALL_ACCEPT => {
            let net = net!();
            sys_accept(task, vfs, &net, args[0], args[1], args[2])
        }
        SYSCALL_CONNECT => {
            let net = net!();
            sys_connect(task, vfs, &net, args[0], args[1], args[2])
        }
        SYSCALL_GETSOCKNAME => {
            let net = net!();
            sys_getsockname(task, vfs, &net, args[0], args[1], args[2])
        }
        SYSCALL_GETPEERNAME => {
            let net = net!();
            sys_getpeername(task, vfs, &net, args[0], args[1], args[2])
        }
        SYSCALL_SENDTO => {
            let net = net!();
            sys_sendto(task, vfs, &net, [args[0], args[1], args[2], args[3], args[4], args[5]])
        }
        SYSCALL_RECVFROM => {
            let net = net!();
            sys_recvfrom(task, vfs, &net, [args[0], args[1], args[2], args[3], args[4], args[5]])
        }
        SYSCALL_SETSOCKOPT => {
            let net = net!();
            sys_set_socket_opt(task, vfs, &net, [args[0], args[1], args[2], args[3], args[4]])
        }
        SYSCALL_GETSOCKOPT => {
            let net = net!();
            sys_get_socket_opt(task, vfs, &net, [args[0], args[1], args[2], args[3], args[4]])
        }
        SYSCALL_SHUTDOWN => {
            let net = net!();
            sys_shutdown(task, vfs, &net, args[0], args[1])
        }
        // 公共 Linux 内存管理与资源限制 syscall。
        SYSCALL_BRK => sys_brk(vfs, task, args[0]),
        SYSCALL_MUNMAP => sys_unmap(task, args[0], args[1]),
        SYSCALL_MMAP => sys_mmap(task, args[0], args[1], args[2], args[3], args[4], args[5]),
        SYSCALL_MPROTECT => sys_mprotect(task, args[0], args[1], args[2]),
        SYSCALL_WAITID => sys_waitid(task, args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_PRLIMIT64 => sys_prlimit64(task, args[0], args[1], args[2], args[3]),
        SYSCALL_MADVISE => sys_madvise(task, args[0], args[1], args[2]),
        SYSCALL_UMASK => sys_umask(task, args[0]),
        SYSCALL_GETRANDOM => sys_random(task, args[0], args[1], args[2]),
        _ => return None,
    };
    Some(result)
}
