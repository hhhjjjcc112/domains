#![no_std]
#![forbid(unsafe_code)]
mod abi;
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

    fn call(&self, syscall_id: usize, args: [usize; 6]) -> AlienResult<isize> {
        let orig_syscall_id = syscall_id;
        let (syscall_id, args) = abi::normalize_syscall_call(syscall_id, args)?;
        let syscall_name = syscall_name(syscall_id);
        // let pid = self.task_domain.current_pid().unwrap();
        let tid = basic::current_tid()?;
        // if syscall_id != SYSCALL_YIELD &&  syscall_id != SYSCALL_WAIT4{
        //     println!("[tid:{:?}] syscall: {}",tid, syscall_name,);
        // }
        if syscall_id == SYSCALL_DOMAIN_TEST {
            let log_info = format!("[tid:{:?}] syscall: {}", tid, SYSCALL_DOMAIN_TEST);
            self.logger.log(
                interface::Level::Info,
                &DVec::from_slice(log_info.as_bytes()),
            )?;
            return Ok(0);
        }

        match syscall_id {
            SYSCALL_EVENTFD2 => sys_eventfd2(&self.vfs_domain, &self.task_domain, args[0], args[1]),
            SYSCALL_EPOLL_CREATE1 => sys_poll_createl(&self.vfs_domain, &self.task_domain, args[0]),
            SYSCALL_EPOLL_CTL => sys_poll_ctl(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
            ),
            SYSCALL_GETCWD => sys_getcwd(&self.vfs_domain, &self.task_domain, args[0], args[1]),
            SYSCALL_DUP => sys_dup(&self.task_domain, args[0]),
            SYSCALL_DUP3 => sys_dup2(&self.task_domain, args[0], args[1]),
            SYSCALL_FCNTL => sys_fcntl(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
            ),
            SYSCALL_IOCTL => sys_ioctl(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
            ),
            SYSCALL_MKDIRAT => sys_mkdirat(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
            ),
            SYSCALL_MOUNT => sys_mount(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
            ),
            SYSCALL_UNLINKAT => sys_unlinkat(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
            ),
            SYSCALL_LINKAT => sys_linkat(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
            ),
            SYSCALL_SYMLINKAT => {
                sys_symlinkat(&self.vfs_domain, &self.task_domain, args[0], args[1], args[2])
            }
            SYSCALL_READLINKAT => sys_readlinkat(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
            ),
            SYSCALL_FTRUNCATE => {
                sys_ftruncate(&self.vfs_domain, &self.task_domain, args[0], args[1])
            }
            SYSCALL_TRUNCATE => sys_truncate(&self.vfs_domain, &self.task_domain, args[0], args[1]),
            SYSCALL_STATFS => sys_statfs(&self.vfs_domain, &self.task_domain, args[0], args[1]),
            SYSCALL_FSTATFS => {
                sys_fstatfs(&self.vfs_domain, &self.task_domain, args[0], args[1])
            }
            SYSCALL_FACCESSAT => sys_faccessat(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
            ),
            SYSCALL_CHDIR => sys_chdir(&self.vfs_domain, &self.task_domain, args[0]),
            SYSCALL_OPENAT => sys_openat(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1] as *const u8,
                args[2],
                args[3],
            ),
            SYSCALL_CLOSE => sys_close(&self.vfs_domain, &self.task_domain, args[0]),
            SYSCALL_PIPE2 => sys_pipe2(&self.task_domain, &self.vfs_domain, args[0], args[1]),
            SYSCALL_GETDENTS64 => sys_getdents64(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
            ),
            SYSCALL_LSEEK => sys_lseek(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
            ),
            SYSCALL_READ => sys_read(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
            ),
            SYSCALL_WRITE => sys_write(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1] as *const u8,
                args[2],
            ),
            SYSCALL_READV => sys_readv(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
            ),
            SYSCALL_WRITEV => sys_writev(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
            ),
            SYSCALL_SENDFILE => sys_sendfile(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
            ),
            SYSCALL_PSELECT6 => sys_pselect6(
                &self.vfs_domain,
                &self.task_domain,
                SelectArgs {
                    nfds: args[0],
                    readfds: args[1],
                    writefds: args[2],
                    exceptfds: args[3],
                    timeout: args[4],
                    sigmask: args[5],
                },
            ),
            SYSCALL_PPOLL => sys_ppoll(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
            ),
            SYSCALL_FSTATAT => sys_fstatat(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1] as *const u8,
                args[2],
                args[3],
            ),
            SYSCALL_FSTAT => sys_fstat(&self.vfs_domain, &self.task_domain, args[0], args[1]),
            SYSCALL_FSYNC => sys_fsync(&self.vfs_domain, &self.task_domain, args[0]),
            SYSCALL_UTIMENSAT => sys_utimensat(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
            ),
            SYSCALL_SETXATTR => sys_setxattr(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
            ),
            SYSCALL_LSETXATTR => sys_lsetxattr(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
            ),
            SYSCALL_FSETXATTR => sys_fsetxattr(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
            ),
            SYSCALL_GETXATTR => sys_getxattr(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
            ),
            SYSCALL_LGETXATTR => sys_lgetxattr(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
            ),
            SYSCALL_FGETXATTR => sys_fgetxattr(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
            ),
            SYSCALL_LISTXATTR => {
                sys_listxattr(&self.vfs_domain, &self.task_domain, args[0], args[1], args[2])
            }
            SYSCALL_LLISTXATTR => {
                sys_llistxattr(&self.vfs_domain, &self.task_domain, args[0], args[1], args[2])
            }
            SYSCALL_FLISTXATTR => {
                sys_flistxattr(&self.vfs_domain, &self.task_domain, args[0], args[1], args[2])
            }
            SYSCALL_REMOVEXATTR => {
                sys_removexattr(&self.vfs_domain, &self.task_domain, args[0], args[1])
            }
            SYSCALL_LREMOVEXATTR => {
                sys_lremovexattr(&self.vfs_domain, &self.task_domain, args[0], args[1])
            }
            SYSCALL_FREMOVEXATTR => {
                sys_fremovexattr(&self.vfs_domain, &self.task_domain, args[0], args[1])
            }
            SYSCALL_EXIT => sys_exit(&self.task_domain, args[0]),
            SYSCALL_EXIT_GROUP => sys_exit_group(&self.task_domain, args[0]),
            SYSCALL_SET_TID_ADDRESS => sys_set_tid_address(&self.task_domain, args[0]),
            #[cfg(target_arch = "x86_64")]
            X86_64_RAW_SYSCALL_SET_TID_ADDRESS => sys_set_tid_address(&self.task_domain, args[0]),
            #[cfg(target_arch = "x86_64")]
            SYSCALL_ARCH_PRCTL => sys_arch_prctl(&self.task_domain, args[0], args[1]),
            SYSCALL_CLOCK_GETTIME => sys_clock_gettime(&self.task_domain, args[0], args[1]),
            SYSCALL_NANOSLEEP => sys_nanosleep(&self.task_domain, args[0], args[1]),
            SYSCALL_YIELD => sys_yield(),
            SYSCALL_FUTEX => sys_futex(
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
                args[5],
            ),
            SYSCALL_SIGALTSTACK => sys_sigaltstack(&self.task_domain, args[0], args[1]),
            SYSCALL_SIGACTION => sys_sigaction(&self.task_domain, args[0], args[1], args[2]),
            SYSCALL_SIGPROCMASK => {
                sys_sigprocmask(&self.task_domain, args[0], args[1], args[2], args[3])
            }
            SYSCALL_SET_PRIORITY => sys_set_priority(&self.task_domain, args[0], args[1], args[2]),
            SYSCALL_GET_PRIORITY => sys_get_priority(&self.task_domain, args[0], args[1]),
            SYSCALL_SETPGID => sys_set_pgid(&self.task_domain),
            SYSCALL_GETPGID => sys_get_pgid(&self.task_domain),
            SYSCALL_SETSID => sys_set_sid(&self.task_domain),
            SYSCALL_UNAME => sys_uname(&self.task_domain, args[0]),
            SYSCALL_GET_TIME_OF_DAY => sys_get_time_of_day(&self.task_domain, args[0]),
            SYSCALL_GETPID => sys_get_pid(&self.task_domain),
            SYSCALL_GETPPID => sys_get_ppid(&self.task_domain),
            SYSCALL_GETUID => sys_getuid(&self.task_domain),
            SYSCALL_GETEUID => sys_get_euid(&self.task_domain),
            SYSCALL_GETGID => sys_get_gid(&self.task_domain),
            SYSCALL_GETEGID => sys_get_egid(&self.task_domain),
            SYSCALL_GETTID => sys_get_tid(),
            SYSCALL_SOCKET => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_socket(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    args[0],
                    args[1],
                    args[2],
                )
            }
            SYSCALL_SOCKETPAIR => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_socket_pair(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    args[0],
                    args[1],
                    args[2],
                    args[3],
                )
            }
            SYSCALL_BIND => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_bind(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    args[0],
                    args[1],
                    args[2],
                )
            }
            SYSCALL_LISTEN => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_listen(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    args[0],
                    args[1],
                )
            }
            SYSCALL_ACCEPT => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_accept(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    args[0],
                    args[1],
                    args[2],
                )
            }
            SYSCALL_CONNECT => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_connect(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    args[0],
                    args[1],
                    args[2],
                )
            }
            SYSCALL_GETSOCKNAME => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_getsockname(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    args[0],
                    args[1],
                    args[2],
                )
            }
            SYSCALL_GETPEERNAME => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_getpeername(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    args[0],
                    args[1],
                    args[2],
                )
            }
            SYSCALL_SENDTO => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_sendto(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    [args[0], args[1], args[2], args[3], args[4], args[5]],
                )
            }
            SYSCALL_RECVFROM => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_recvfrom(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    [args[0], args[1], args[2], args[3], args[4], args[5]],
                )
            }
            SYSCALL_SETSOCKOPT => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_set_socket_opt(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    [args[0], args[1], args[2], args[3], args[4]],
                )
            }
            SYSCALL_GETSOCKOPT => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_get_socket_opt(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    [args[0], args[1], args[2], args[3], args[4]],
                )
            }
            SYSCALL_SHUTDOWN => {
                let net_stack_domain = self.net_stack_domain()?;
                sys_shutdown(
                    &self.task_domain,
                    &self.vfs_domain,
                    &net_stack_domain,
                    args[0],
                    args[1],
                )
            }
            SYSCALL_BRK => sys_brk(&self.vfs_domain, &self.task_domain, args[0]),
            SYSCALL_MUNMAP => sys_unmap(&self.task_domain, args[0], args[1]),
            SYSCALL_CLONE => sys_clone(
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
            ),
            SYSCALL_EXECVE => sys_execve(&self.task_domain, args[0], args[1], args[2]),
            SYSCALL_MMAP => sys_mmap(
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
                args[5],
            ),
            SYSCALL_MPROTECT => sys_mprotect(&self.task_domain, args[0], args[1], args[2]),
            SYSCALL_WAIT4 => sys_wait4(&self.task_domain, args[0], args[1], args[2], args[3]),
            SYSCALL_WAITID => {
                sys_waitid(&self.task_domain, args[0], args[1], args[2], args[3], args[4])
            }
            SYSCALL_PRLIMIT => sys_prlimit64(&self.task_domain, args[0], args[1], args[2], args[3]),
            SYSCALL_MADVISE => sys_madvise(&self.task_domain, args[0], args[1], args[2]),
            SYSCALL_RENAMEAT2 => sys_renameat2(
                &self.vfs_domain,
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
            ),
            SYSCALL_GETRANDOM => sys_random(&self.task_domain, args[0], args[1], args[2]),
            SYSCALL_LOAD_DOMAIN => sys_load_domain(
                &self.task_domain,
                &self.vfs_domain,
                args[0],
                args[1] as u8,
                args[2],
                args[3],
            ),
            SYSCALL_REPLACE_DOMAIN => sys_replace_domain(
                &self.task_domain,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4] as u8,
            ),
            SYSCALL_FRAMEBUFFER => sys_framebuffer(&self.task_domain, self.gpu_domain.as_ref()),
            SYSCALL_FRAMEBUFFER_FLUSH => sys_framebuffer_flush(self.gpu_domain.as_ref()),
            SYSCALL_EVENT_GET => sys_event_get(
                &self.task_domain,
                self.input_domain.as_slice(),
                args[0],
                args[1],
            ),
            _ => {
                #[cfg(target_arch = "x86_64")]
                if is_linux_x86_64_abi_syscall(orig_syscall_id) {
                    log::warn!(
                        "[x86 linux-abi] unsupported syscall raw={:#x}",
                        raw_syscall_id(orig_syscall_id)
                    );
                    return Err(AlienError::ENOSYS);
                }
                panic!("syscall [{}: {}] not found", syscall_id, syscall_name)
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
    for name in [
        "gpu",
        "gpu-1",
        "virtio_gpu-1",
        "virtio_gpu",
    ] {
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
