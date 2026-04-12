#![no_std]
#![forbid(unsafe_code)]
mod domain;
mod fs;
mod gui;
mod mm;
mod signal;
mod socket;
mod system;
mod task;
mod time;

extern crate alloc;
extern crate log;

use alloc::{boxed::Box, format, sync::Arc, vec, vec::Vec};

use basic::{constants::*, println, AlienError, AlienResult};
use interface::*;
use shared_heap::DVec;

use crate::{domain::*, fs::*, gui::*, mm::*, signal::*, socket::*, system::*, task::*, time::*};

#[derive(Debug)]
struct SysCallDomainImpl {
    vfs_domain: Arc<dyn VfsDomain>,
    task_domain: Arc<dyn TaskDomain>,
    logger: Arc<dyn LogDomain>,
    net_stack_domain: Option<Arc<dyn NetDomain>>,
    gpu_domain: Option<Arc<dyn GpuDomain>>,
    input_domain: Vec<Arc<dyn BufInputDomain>>,
}

impl SysCallDomainImpl {
    pub fn new(
        vfs_domain: Arc<dyn VfsDomain>,
        task_domain: Arc<dyn TaskDomain>,
        logger: Arc<dyn LogDomain>,
        net_stack_domain: Option<Arc<dyn NetDomain>>,
        gpu_domain: Option<Arc<dyn GpuDomain>>,
        input_domain: Vec<Arc<dyn BufInputDomain>>,
    ) -> Self {
        Self {
            vfs_domain,
            task_domain,
            logger,
            net_stack_domain,
            gpu_domain,
            input_domain,
        }
    }

    fn net_stack_domain(&self) -> AlienResult<Arc<dyn NetDomain>> {
        if let Some(net_stack_domain) = self.net_stack_domain.as_ref() {
            return Ok(net_stack_domain.clone());
        }
        match basic::get_domain("net_stack") {
            Some(DomainType::NetDomain(net_stack_domain)) => Ok(net_stack_domain),
            _ => Err(AlienError::ENOSYS),
        }
    }
}

impl Basic for SysCallDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl SysCallDomain for SysCallDomainImpl {
    fn init(&self) -> AlienResult<()> {
        let log_info = "syscall domain test log domain.";
        self.logger.log(
            interface::Level::Info,
            &DVec::from_slice(log_info.as_bytes()),
        )?;
        println!("syscall domain init");
        Ok(())
    }

    /// 统一接收原始 syscall，直接按子系统分发；`syscall_id` 是原始号，`args` 是 6 个原始参数。
    fn call(&self, syscall_id: usize, args: [usize; 6]) -> AlienResult<isize> {
        let _orig_syscall_id = syscall_id;
        let tid = basic::current_tid()?;
        if syscall_id == SYSCALL_DOMAIN_TEST {
            let log_info = format!("[tid:{:?}] syscall: {}", tid, SYSCALL_DOMAIN_TEST);
            self.logger.log(
                interface::Level::Info,
                &DVec::from_slice(log_info.as_bytes()),
            )?;
            return Ok(0);
        }

        let task = &self.task_domain;
        let vfs = &self.vfs_domain;
        let gpu = self.gpu_domain.as_ref();
        let input = self.input_domain.as_slice();

        #[cfg(target_arch = "x86_64")]
        {
            match syscall_id {
                // open：x86_64 旧号兼容到 openat(AT_FDCWD, ...)。
                SYSCALL_OPEN => return sys_openat(vfs, task, AT_FDCWD as usize, args[0] as *const u8, args[1], args[2]),
                // access：x86_64 旧号兼容到 faccessat(AT_FDCWD, ..., 0)。
                SYSCALL_ACCESS => return sys_faccessat(vfs, task, AT_FDCWD as usize, args[0], args[1], 0),
                // stat：x86_64 旧号兼容到 newfstatat(AT_FDCWD, ..., 0)。
                SYSCALL_STAT => return sys_fstatat(vfs, task, AT_FDCWD as usize, args[0] as *const u8, args[1], 0),
                // lstat：x86_64 旧号兼容到 newfstatat(AT_FDCWD, ..., AT_SYMLINK_NOFOLLOW)。
                SYSCALL_LSTAT => return sys_fstatat(vfs, task, AT_FDCWD as usize, args[0] as *const u8, args[1], pconst::io::StatFlags::AT_SYMLINK_NOFOLLOW.bits() as usize),
                // mkdir：x86_64 旧号兼容到 mkdirat(AT_FDCWD, ...)。
                SYSCALL_MKDIR => return sys_mkdirat(vfs, task, AT_FDCWD as usize, args[0], args[1]),
                // pipe：x86_64 旧号兼容到 pipe2(pipefd, 0)。
                SYSCALL_PIPE => return sys_pipe2(task, vfs, args[0], 0),
                // poll：x86_64 旧号兼容到内部 ppoll 毫秒路径。
                SYSCALL_POLL => return sys_ppoll(vfs, task, args[0], args[1], args[2], PPOLL_FROM_POLL_SIGMASK),
                // arch_prctl：x86_64 直接走 FS/GS 基址控制。
                SYSCALL_ARCH_PRCTL => return sys_arch_prctl(task, args[0], args[1]),
                // clone：x86_64 raw ABI 需要把 child_tid 和 tls 对调后再进入统一 helper。
                SYSCALL_CLONE => return sys_clone(task, args[0], args[1], args[2], args[4], args[3]),
                // fork：x86_64 旧号兼容到 clone(SIGCHLD, ...)。
                SYSCALL_FORK => return sys_clone(task, basic::constants::signal::SignalNumber::SIGCHLD as usize, 0, 0, 0, 0),
                // vfork：x86_64 旧号兼容到 clone(CLONE_VFORK | CLONE_VM | SIGCHLD, ...)。
                SYSCALL_VFORK => return sys_clone(task, (basic::constants::task::CloneFlags::CLONE_VFORK | basic::constants::task::CloneFlags::CLONE_VM).bits() as usize | basic::constants::signal::SignalNumber::SIGCHLD as usize, 0, 0, 0, 0),
                // renameat：x86_64 旧号兼容到 renameat2(..., 0)。
                SYSCALL_RENAMEAT => return sys_renameat2(vfs, task, args[0], args[1], args[2], args[3], 0),
                _ => {}
            }
        }

        match syscall_id {
            // eventfd2：创建 eventfd。
            SYSCALL_EVENTFD2 => sys_eventfd2(vfs, task, args[0], args[1]),
            // epoll_create1：创建 epoll 实例。
            SYSCALL_EPOLL_CREATE1 => sys_poll_createl(vfs, task, args[0]),
            // epoll_ctl：管理 epoll 关注项。
            SYSCALL_EPOLL_CTL => sys_poll_ctl(vfs, task, args[0], args[1], args[2], args[3]),
            // getcwd：读取当前工作目录。
            SYSCALL_GETCWD => sys_getcwd(vfs, task, args[0], args[1]),
            // dup：复制文件描述符。
            SYSCALL_DUP => sys_dup(task, args[0]),
            // dup3：复制文件描述符并处理最小 O_CLOEXEC 语义。
            SYSCALL_DUP3 => sys_dup3(vfs, task, args[0], args[1], args[2]),
            // fcntl：处理文件描述符控制命令。
            SYSCALL_FCNTL => sys_fcntl(vfs, task, args[0], args[1], args[2]),
            // ioctl：处理设备控制命令。
            SYSCALL_IOCTL => sys_ioctl(vfs, task, args[0], args[1], args[2]),
            // mkdirat：相对目录创建目录。
            SYSCALL_MKDIRAT => sys_mkdirat(vfs, task, args[0], args[1], args[2]),
            // mount：挂载文件系统。
            SYSCALL_MOUNT => sys_mount(vfs, task, args[0], args[1], args[2], args[3], args[4]),
            // unlinkat：相对目录删除目录项。
            SYSCALL_UNLINKAT => sys_unlinkat(vfs, task, args[0], args[1], args[2]),
            // linkat：创建硬链接。
            SYSCALL_LINKAT => sys_linkat(vfs, task, args[0], args[1], args[2], args[3], args[4]),
            // symlinkat：创建符号链接。
            SYSCALL_SYMLINKAT => sys_symlinkat(vfs, task, args[0], args[1], args[2]),
            // readlinkat：读取符号链接目标。
            SYSCALL_READLINKAT => sys_readlinkat(vfs, task, args[0], args[1], args[2], args[3]),
            // renameat2：重命名目录项。
            SYSCALL_RENAMEAT2 => sys_renameat2(vfs, task, args[0], args[1], args[2], args[3], args[4]),
            // ftruncate：按 fd 截断文件。
            SYSCALL_FTRUNCATE => sys_ftruncate(vfs, task, args[0], args[1]),
            // truncate：按路径截断文件。
            SYSCALL_TRUNCATE => sys_truncate(vfs, task, args[0], args[1]),
            // statfs：按路径读取文件系统统计信息。
            SYSCALL_STATFS => sys_statfs(vfs, task, args[0], args[1]),
            // fstatfs：按 fd 读取文件系统统计信息。
            SYSCALL_FSTATFS => sys_fstatfs(vfs, task, args[0], args[1]),
            // faccessat：按路径检查访问权限。
            SYSCALL_FACCESSAT => sys_faccessat(vfs, task, args[0], args[1], args[2], args[3]),
            // chdir：切换当前工作目录。
            SYSCALL_CHDIR => sys_chdir(vfs, task, args[0]),
            // openat：按相对目录打开文件。
            SYSCALL_OPENAT => sys_openat(vfs, task, args[0], args[1] as *const u8, args[2], args[3]),
            // close：关闭文件描述符。
            SYSCALL_CLOSE => sys_close(vfs, task, args[0]),
            // pipe2：创建管道。
            SYSCALL_PIPE2 => sys_pipe2(task, vfs, args[0], args[1]),
            // getdents64：读取目录项。
            SYSCALL_GETDENTS64 => sys_getdents64(vfs, task, args[0], args[1], args[2]),
            // lseek：调整文件偏移。
            SYSCALL_LSEEK => sys_lseek(vfs, task, args[0], args[1], args[2]),
            // read：从文件读取数据。
            SYSCALL_READ => sys_read(vfs, task, args[0], args[1], args[2]),
            // write：向文件写入数据。
            SYSCALL_WRITE => sys_write(vfs, task, args[0], args[1] as *const u8, args[2]),
            // readv：分散读取。
            SYSCALL_READV => sys_readv(vfs, task, args[0], args[1], args[2]),
            // writev：聚集写入。
            SYSCALL_WRITEV => sys_writev(vfs, task, args[0], args[1], args[2]),
            // sendfile：文件到文件拷贝。
            SYSCALL_SENDFILE => sys_sendfile(vfs, task, args[0], args[1], args[2], args[3]),
            // pselect6：select 兼容入口。
            SYSCALL_PSELECT6 => sys_pselect6(vfs, task, SelectArgs { nfds: args[0], readfds: args[1], writefds: args[2], exceptfds: args[3], timeout: args[4], sigmask: args[5] }),
            // ppoll：riscv64 直接使用原生 ppoll 号。
            #[cfg(target_arch = "riscv64")]
            SYSCALL_PPOLL => sys_ppoll(vfs, task, args[0], args[1], args[2], args[3]),
            // newfstatat：按路径读取文件属性。
            SYSCALL_NEWFSTATAT => sys_fstatat(vfs, task, args[0], args[1] as *const u8, args[2], args[3]),
            // fstat：按 fd 读取文件属性。
            SYSCALL_FSTAT => sys_fstat(vfs, task, args[0], args[1]),
            // fsync：同步文件内容。
            SYSCALL_FSYNC => sys_fsync(vfs, task, args[0]),
            // utimensat：更新文件时间戳。
            SYSCALL_UTIMENSAT => sys_utimensat(vfs, task, args[0], args[1], args[2], args[3]),
            // setxattr：设置路径扩展属性。
            SYSCALL_SETXATTR => sys_setxattr(vfs, task, args[0], args[1], args[2], args[3], args[4]),
            // lsetxattr：设置符号链接扩展属性。
            SYSCALL_LSETXATTR => sys_lsetxattr(vfs, task, args[0], args[1], args[2], args[3], args[4]),
            // fsetxattr：设置 fd 扩展属性。
            SYSCALL_FSETXATTR => sys_fsetxattr(vfs, task, args[0], args[1], args[2], args[3], args[4]),
            // getxattr：读取路径扩展属性。
            SYSCALL_GETXATTR => sys_getxattr(vfs, task, args[0], args[1], args[2], args[3]),
            // lgetxattr：读取符号链接扩展属性。
            SYSCALL_LGETXATTR => sys_lgetxattr(vfs, task, args[0], args[1], args[2], args[3]),
            // fgetxattr：读取 fd 扩展属性。
            SYSCALL_FGETXATTR => sys_fgetxattr(vfs, task, args[0], args[1], args[2], args[3]),
            // listxattr：列出路径扩展属性。
            SYSCALL_LISTXATTR => sys_listxattr(vfs, task, args[0], args[1], args[2]),
            // llistxattr：列出符号链接扩展属性。
            SYSCALL_LLISTXATTR => sys_llistxattr(vfs, task, args[0], args[1], args[2]),
            // flistxattr：列出 fd 扩展属性。
            SYSCALL_FLISTXATTR => sys_flistxattr(vfs, task, args[0], args[1], args[2]),
            // removexattr：删除路径扩展属性。
            SYSCALL_REMOVEXATTR => sys_removexattr(vfs, task, args[0], args[1]),
            // lremovexattr：删除符号链接扩展属性。
            SYSCALL_LREMOVEXATTR => sys_lremovexattr(vfs, task, args[0], args[1]),
            // fremovexattr：删除 fd 扩展属性。
            SYSCALL_FREMOVEXATTR => sys_fremovexattr(vfs, task, args[0], args[1]),

            // exit：退出当前任务。
            SYSCALL_EXIT => sys_exit(task, args[0]),
            // exit_group：退出整个线程组。
            SYSCALL_EXIT_GROUP => sys_exit_group(task, args[0]),
            // set_tid_address：设置线程退出回写地址。
            SYSCALL_SET_TID_ADDRESS => sys_set_tid_address(task, args[0]),
            // clock_gettime：读取指定时钟。
            SYSCALL_CLOCK_GETTIME => sys_clock_gettime(task, args[0], args[1]),
            // nanosleep：相对时间休眠。
            SYSCALL_NANOSLEEP => sys_nanosleep(task, args[0], args[1]),
            // sched_yield：主动让出 CPU。
            SYSCALL_SCHED_YIELD => sys_yield(),
            // futex：执行 futex 同步操作。
            SYSCALL_FUTEX => sys_futex(task, args[0], args[1], args[2], args[3], args[4], args[5]),
            // sigaltstack：设置或读取备用信号栈。
            SYSCALL_SIGALTSTACK => sys_sigaltstack(task, args[0], args[1]),
            // rt_sigaction：安装或读取信号处理动作。
            SYSCALL_RT_SIGACTION => sys_sigaction(task, args[0], args[1], args[2], args[3]),
            // rt_sigprocmask：设置或读取信号掩码。
            SYSCALL_RT_SIGPROCMASK => sys_sigprocmask(task, args[0], args[1], args[2], args[3]),
            // setpriority：设置调度优先级。
            SYSCALL_SETPRIORITY => sys_set_priority(task, args[0], args[1], args[2]),
            // getpriority：读取调度优先级。
            SYSCALL_GETPRIORITY => sys_get_priority(task, args[0], args[1]),
            // setpgid：设置进程组 ID。
            SYSCALL_SETPGID => sys_set_pgid(task, args[0], args[1]),
            // getpgid：读取进程组 ID。
            SYSCALL_GETPGID => sys_get_pgid(task, args[0]),
            // setsid：创建新会话。
            SYSCALL_SETSID => sys_set_sid(task),
            // uname：读取系统标识信息。
            SYSCALL_UNAME => sys_uname(task, args[0]),
            // gettimeofday：读取墙上时钟时间。
            SYSCALL_GETTIMEOFDAY => sys_get_time_of_day(task, args[0], args[1]),
            // getpid：读取当前进程 ID。
            SYSCALL_GETPID => sys_get_pid(task),
            // getppid：读取父进程 ID。
            SYSCALL_GETPPID => sys_get_ppid(task),
            // getuid：读取真实用户 ID。
            SYSCALL_GETUID => sys_getuid(task),
            // geteuid：读取有效用户 ID。
            SYSCALL_GETEUID => sys_get_euid(task),
            // getgid：读取真实组 ID。
            SYSCALL_GETGID => sys_get_gid(task),
            // getegid：读取有效组 ID。
            SYSCALL_GETEGID => sys_get_egid(task),
            // gettid：读取当前线程 ID。
            SYSCALL_GETTID => sys_get_tid(),
            // clone：riscv64 按原始参数顺序直通统一 helper。
            #[cfg(target_arch = "riscv64")]
            SYSCALL_CLONE => sys_clone(task, args[0], args[1], args[2], args[3], args[4]),
            // clone3：当前未实现。
            SYSCALL_CLONE3 => Err(AlienError::ENOSYS),
            // execve：执行新程序映像。
            SYSCALL_EXECVE => sys_execve(task, args[0], args[1], args[2]),
            // wait4：等待子进程结束。
            SYSCALL_WAIT4 => sys_wait4(task, args[0], args[1], args[2], args[3]),

            // socket：创建套接字。
            SYSCALL_SOCKET => { let net = self.net_stack_domain()?; sys_socket(task, vfs, &net, args[0], args[1], args[2]) }
            // socketpair：创建一对已连接套接字。
            SYSCALL_SOCKETPAIR => { let net = self.net_stack_domain()?; sys_socket_pair(task, vfs, &net, args[0], args[1], args[2], args[3]) }
            // bind：绑定本地地址。
            SYSCALL_BIND => { let net = self.net_stack_domain()?; sys_bind(task, vfs, &net, args[0], args[1], args[2]) }
            // listen：切换到监听状态。
            SYSCALL_LISTEN => { let net = self.net_stack_domain()?; sys_listen(task, vfs, &net, args[0], args[1]) }
            // accept：接受入站连接。
            SYSCALL_ACCEPT => { let net = self.net_stack_domain()?; sys_accept(task, vfs, &net, args[0], args[1], args[2]) }
            // connect：主动连接远端地址。
            SYSCALL_CONNECT => { let net = self.net_stack_domain()?; sys_connect(task, vfs, &net, args[0], args[1], args[2]) }
            // getsockname：读取本地地址。
            SYSCALL_GETSOCKNAME => { let net = self.net_stack_domain()?; sys_getsockname(task, vfs, &net, args[0], args[1], args[2]) }
            // getpeername：读取对端地址。
            SYSCALL_GETPEERNAME => { let net = self.net_stack_domain()?; sys_getpeername(task, vfs, &net, args[0], args[1], args[2]) }
            // sendto：发送数据到指定地址。
            SYSCALL_SENDTO => { let net = self.net_stack_domain()?; sys_sendto(task, vfs, &net, [args[0], args[1], args[2], args[3], args[4], args[5]]) }
            // recvfrom：从指定地址接收数据。
            SYSCALL_RECVFROM => { let net = self.net_stack_domain()?; sys_recvfrom(task, vfs, &net, [args[0], args[1], args[2], args[3], args[4], args[5]]) }
            // setsockopt：设置套接字选项。
            SYSCALL_SETSOCKOPT => { let net = self.net_stack_domain()?; sys_set_socket_opt(task, vfs, &net, [args[0], args[1], args[2], args[3], args[4]]) }
            // getsockopt：读取套接字选项。
            SYSCALL_GETSOCKOPT => { let net = self.net_stack_domain()?; sys_get_socket_opt(task, vfs, &net, [args[0], args[1], args[2], args[3], args[4]]) }
            // shutdown：关闭套接字收发方向。
            SYSCALL_SHUTDOWN => { let net = self.net_stack_domain()?; sys_shutdown(task, vfs, &net, args[0], args[1]) }

            // brk：调整进程数据段末端。
            SYSCALL_BRK => sys_brk(vfs, task, args[0]),
            // munmap：解除内存映射。
            SYSCALL_MUNMAP => sys_unmap(task, args[0], args[1]),
            // mmap：建立内存映射。
            SYSCALL_MMAP => sys_mmap(task, args[0], args[1], args[2], args[3], args[4], args[5]),
            // mprotect：修改映射权限。
            SYSCALL_MPROTECT => sys_mprotect(task, args[0], args[1], args[2]),
            // waitid：按 idtype 等待子进程。
            SYSCALL_WAITID => sys_waitid(task, args[0], args[1], args[2], args[3], args[4]),
            // prlimit64：设置或读取资源限制。
            SYSCALL_PRLIMIT64 => sys_prlimit64(task, args[0], args[1], args[2], args[3]),
            // madvise：提供内存访问建议。
            SYSCALL_MADVISE => sys_madvise(task, args[0], args[1], args[2]),

            // getrandom：生成随机字节。
            SYSCALL_GETRANDOM => sys_random(task, args[0], args[1], args[2]),
            // load_domain：加载并注册新域。
            SYSCALL_LOAD_DOMAIN => sys_load_domain(task, vfs, args[0], args[1] as u8, args[2], args[3]),
            // replace_domain：替换已有域实现。
            SYSCALL_REPLACE_DOMAIN => sys_replace_domain(task, args[0], args[1], args[2], args[3], args[4] as u8),
            // framebuffer：获取帧缓冲映射。
            SYSCALL_FRAMEBUFFER => sys_framebuffer(task, gpu),
            // framebuffer_flush：刷新帧缓冲。
            SYSCALL_FRAMEBUFFER_FLUSH => sys_framebuffer_flush(gpu),
            // event_get：读取输入事件。
            SYSCALL_EVENT_GET => sys_event_get(task, input, args[0], args[1]),
            _ => {
                log::warn!("unsupported syscall raw={:#x}", _orig_syscall_id);
                Err(AlienError::ENOSYS)
            }
        }
    }
}
define_unwind_for_SysCallDomain!(SysCallDomainImpl);

pub fn main() -> Box<dyn SysCallDomain> {
    let vfs_domain = basic::get_domain("vfs").unwrap();
    let vfs_domain = match vfs_domain {
        DomainType::VfsDomain(vfs_domain) => vfs_domain,
        _ => panic!("vfs domain not found"),
    };
    let task_domain = basic::get_domain("task").unwrap();
    let task_domain = match task_domain {
        DomainType::TaskDomain(task_domain) => task_domain,
        _ => panic!("task domain not found"),
    };

    let logger = basic::get_domain("logger").unwrap();
    let logger = match logger {
        DomainType::LogDomain(logger) => logger,
        _ => panic!("logger domain not found"),
    };

    let net_stack_domain = match basic::get_domain("net_stack") {
        Some(DomainType::NetDomain(net_stack_domain)) => Some(net_stack_domain),
        Some(_) => {
            log::warn!("net_stack domain type mismatch, skip");
            None
        }
        None => {
            log::warn!("net_stack domain not found, skip");
            None
        }
    };

    let mut gpu_domain = None;
    for name in ["gpu", "gpu-1", "virtio_gpu-1", "virtio_gpu"] {
        if let Some(DomainType::GpuDomain(domain)) = basic::get_domain(name) {
            gpu_domain = Some(domain);
            break;
        }
    }

    let mut input_domains = vec![];
    let mut count = 1;
    loop {
        let name = format!("buf_input-{}", count);
        let buf_input_domain = basic::get_domain(&name);
        match buf_input_domain {
            Some(DomainType::BufInputDomain(buf_input_domain)) => {
                input_domains.push(buf_input_domain);
                count += 1;
            }
            _ => {
                break;
            }
        }
    }
    println!("syscall get {} input domain", count - 1);
    Box::new(UnwindWrap::new(SysCallDomainImpl::new(
        vfs_domain,
        task_domain,
        logger,
        net_stack_domain,
        gpu_domain,
        input_domains,
    )))
}
