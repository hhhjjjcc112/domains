use basic::{constants::*, AlienResult};

pub fn normalize_syscall_call(
    syscall_id: usize,
    args: [usize; 6],
) -> AlienResult<(usize, [usize; 6])> {
    if !is_linux_x86_64_abi_syscall(syscall_id) {
        return Ok((syscall_id, args));
    }

    let orig_syscall_id = raw_syscall_id(syscall_id);
    let mut mapped_args = args;
    let mut mapped_syscall_id = match orig_syscall_id {
        // 基础文件与内存
        0x0 => SYSCALL_READ,
        0x1 => SYSCALL_WRITE,
        0x2 => SYSCALL_OPENAT,
        0x3 => SYSCALL_CLOSE,
        0x4 => SYSCALL_FSTATAT,
        0x5 => SYSCALL_FSTAT,
        0x7 => SYSCALL_PPOLL,
        0x8 => SYSCALL_LSEEK,
        0x9 => SYSCALL_MMAP,
        0xa => SYSCALL_MPROTECT,
        0xb => SYSCALL_MUNMAP,
        0xc => SYSCALL_BRK,
        0xd => SYSCALL_SIGACTION,
        0xe => SYSCALL_SIGPROCMASK,
        0x10 => SYSCALL_IOCTL,
        0x13 => SYSCALL_READV,
        0x14 => SYSCALL_WRITEV,
        0x18 => SYSCALL_YIELD,
        0x1c => SYSCALL_MADVISE,
        0x20 => SYSCALL_DUP,
        0x27 => SYSCALL_GETPID,
        0x28 => SYSCALL_SENDFILE,
        0x29 => SYSCALL_SOCKET,
        0x2a => SYSCALL_CONNECT,
        0x2b => SYSCALL_ACCEPT,
        0x2c => SYSCALL_SENDTO,
        0x2d => SYSCALL_RECVFROM,
        0x30 => SYSCALL_SHUTDOWN,
        0x31 => SYSCALL_BIND,
        0x32 => SYSCALL_LISTEN,
        0x33 => SYSCALL_GETSOCKNAME,
        0x34 => SYSCALL_GETPEERNAME,
        0x35 => SYSCALL_SOCKETPAIR,
        0x36 => SYSCALL_SETSOCKOPT,
        0x37 => SYSCALL_GETSOCKOPT,
        0x38 => SYSCALL_CLONE,
        0x3b => SYSCALL_EXECVE,
        0x3c => SYSCALL_EXIT,
        0x3d => SYSCALL_WAIT4,
        0x3f => SYSCALL_UNAME,
        0x48 => SYSCALL_FCNTL,
        0x4a => SYSCALL_FSYNC,
        0x4d => SYSCALL_FTRUNCATE,
        0x4f => SYSCALL_GETCWD,
        0x50 => SYSCALL_CHDIR,
        0x60 => SYSCALL_GET_TIME_OF_DAY,
        0x66 => SYSCALL_GETUID,
        0x68 => SYSCALL_GETGID,
        0x6b => SYSCALL_GETEUID,
        0x6c => SYSCALL_GETEGID,
        0x6d => SYSCALL_SETPGID,
        0x6e => SYSCALL_GETPPID,
        0x6f => SYSCALL_GETPGID,
        0x70 => SYSCALL_SETSID,
        0x79 => SYSCALL_GETPGID,
        0x83 => SYSCALL_SIGALTSTACK,
        0x8c => SYSCALL_GET_PRIORITY,
        0x8d => SYSCALL_SET_PRIORITY,
        0xba => SYSCALL_GETTID,
        0xca => SYSCALL_FUTEX,
        0xd9 => SYSCALL_GETDENTS64,
        0xda => SYSCALL_SET_TID_ADDRESS,
        0xe4 => SYSCALL_CLOCK_GETTIME,
        0xe7 => SYSCALL_EXIT_GROUP,
        0xe9 => SYSCALL_EPOLL_CTL,
        0x101 => SYSCALL_OPENAT,
        0x102 => SYSCALL_MKDIRAT,
        0x106 => SYSCALL_FSTATAT,
        0x107 => SYSCALL_UNLINKAT,
        0x10d => SYSCALL_FACCESSAT,
        0x10e => SYSCALL_PSELECT6,
        0x10f => SYSCALL_PPOLL,
        0x118 => SYSCALL_UTIMENSAT,
        0x122 => SYSCALL_EVENTFD2,
        0x123 => SYSCALL_EPOLL_CREATE1,
        0x124 => SYSCALL_DUP3,
        0x125 => SYSCALL_PIPE2,
        0x12e => SYSCALL_PRLIMIT,
        0x13c => SYSCALL_RENAMEAT2,
        0x13e => SYSCALL_GETRANDOM,
        #[cfg(target_arch = "x86_64")]
        X86_64_RAW_SYSCALL_ARCH_PRCTL => SYSCALL_ARCH_PRCTL,
        _ => return Ok((orig_syscall_id, args)),
    };

    if orig_syscall_id == 0x2 {
        mapped_args[0] = AT_FDCWD as usize;
        mapped_args[1] = args[0];
        mapped_args[2] = args[1];
        mapped_args[3] = args[2];
        mapped_args[4] = 0;
        mapped_args[5] = 0;
        mapped_syscall_id = SYSCALL_OPENAT;
    }

    if orig_syscall_id == 0x38 {
        mapped_args[0] = args[0];
        mapped_args[1] = args[1];
        mapped_args[2] = args[2];
        mapped_args[3] = args[4];
        mapped_args[4] = args[3];
        mapped_args[5] = 0;
        mapped_syscall_id = SYSCALL_CLONE;
    }

    if orig_syscall_id == 0x4 {
        mapped_args[0] = AT_FDCWD as usize;
        mapped_args[1] = args[0];
        mapped_args[2] = args[1];
        mapped_args[3] = 0;
        mapped_args[4] = 0;
        mapped_args[5] = 0;
        mapped_syscall_id = SYSCALL_FSTATAT;
    }

    if orig_syscall_id == 0x6f {
        mapped_args[0] = 0;
        mapped_args[1] = 0;
        mapped_args[2] = 0;
        mapped_args[3] = 0;
        mapped_args[4] = 0;
        mapped_args[5] = 0;
        mapped_syscall_id = SYSCALL_GETPGID;
    }

    if orig_syscall_id == 0x7 {
        let timeout_ms = args[2] as isize;
        mapped_args[2] = if timeout_ms < 0 { 0 } else { timeout_ms as usize };
        mapped_args[3] = usize::MAX;
        mapped_args[4] = 0;
        mapped_args[5] = 0;
        mapped_syscall_id = SYSCALL_PPOLL;
    }

    Ok((mapped_syscall_id, mapped_args))
}