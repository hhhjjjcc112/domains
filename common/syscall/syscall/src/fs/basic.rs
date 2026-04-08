use alloc::{sync::Arc, vec};
use core::cmp::min;

use basic::{
    config::MAX_FD_NUM,
    constants::{io::*, time::TimeSpec, LinuxErrno, AT_FDCWD},
    println_color,
    time::{read_time_us, TimeNow, ToClock},
    AlienError, AlienResult,
};
use bit_field::BitField;
use interface::{TaskDomain, VfsDomain};
use log::{debug, info};
use pod::Pod;
use shared_heap::{DBox, DVec};
use vfscore::utils::{VfsFileStat, VfsFsStat, VfsPollEvents};

use crate::fs::user_path_at;

pub fn sys_openat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    dirfd: usize,
    path: *const u8,
    flags: usize,
    mode: usize,
) -> AlienResult<isize> {
    if path.is_null() {
        return Err(AlienError::EFAULT);
    }
    let mut tmp_buf = DVec::<u8>::new_uninit(256);
    let len;
    (tmp_buf, len) = task_domain.read_string_from_user(path as usize, tmp_buf)?;
    let path = core::str::from_utf8(&tmp_buf.as_slice()[..len]).unwrap();
    info!(
        "<sys_openat> path: {:?} flags: {:?} mode: {:?}",
        path, flags, mode
    );
    let (_, current_root) = user_path_at(task_domain, dirfd as isize, path)?;
    let file = vfs.vfs_open(current_root, &tmp_buf, len, mode as _, flags as _)?;
    let fd = task_domain.add_fd(file)?;
    Ok(fd as isize)
}

pub fn sys_close(
    _vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
) -> AlienResult<isize> {
    info!("<sys_close> fd: {:?}", fd);
    let _file = task_domain.remove_fd(fd)?;
    Ok(0)
}

pub fn sys_write(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    buf: *const u8,
    len: usize,
) -> AlienResult<isize> {
    let file = match task_domain.get_fd(fd) {
        Ok(file) => file,
        Err(err) => {
            if fd <= 2 {
                println_color!(
                    31,
                    "[sys_write_trace] get_fd failed: fd={}, len={}, err={:?}",
                    fd,
                    len,
                    err
                );
            }
            return Err(err);
        }
    };
    if len == 0 {
        return Ok(0);
    }
    let mut tmp_buf = DVec::<u8>::new_uninit(len);
    task_domain.copy_from_user(buf as usize, tmp_buf.as_mut_slice())?;
    let w = vfs.vfs_write(file, &tmp_buf, len);
    if let Err(err) = &w {
        if fd <= 2 {
            println_color!(
                31,
                "[sys_write_trace] vfs_write failed: fd={}, len={}, err={:?}",
                fd,
                len,
                err
            );
        }
    }
    w.map(|x| x as isize)
}

pub fn sys_read(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    buf: usize,
    len: usize,
) -> AlienResult<isize> {
    info!("<sys_read> fd: {:?} buf: {:#x} len: {:?}", fd, buf, len);
    // let now = read_time_us();
    let file = task_domain.get_fd(fd)?;
    if len == 0 {
        return Ok(0);
    }
    // let get_file_time = read_time_us();
    // todo!(if DVec.len is 0, talc will panic)
    let mut tmp_buf = DVec::<u8>::new_uninit(len);
    let r;
    (tmp_buf, r) = vfs.vfs_read(file, tmp_buf)?;
    // let read_file_time = read_time_us();
    task_domain.copy_to_user(buf, &tmp_buf.as_slice()[..r])?;
    // let copy_to_user_time = read_time_us();
    // if len == 4096 {
    //     println_color!(
    //         31,
    //         "sys_read: get_file_time: {}us, read_file_time: {}us, copy_to_user_time: {}us",
    //         get_file_time - now,
    //         read_file_time - get_file_time,
    //         copy_to_user_time - read_file_time
    //     );
    // }
    Ok(r as isize)
}

pub fn sys_readv(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    iov: usize,
    iovcnt: usize,
) -> AlienResult<isize> {
    info!(
        "<sys_readv> fd: {:?} iov: {:#x} iovcnt: {:?}",
        fd, iov, iovcnt
    );
    let file = task_domain.get_fd(fd)?;
    let mut count = 0;
    for i in 0..iovcnt {
        let ptr = iov + i * core::mem::size_of::<IoVec>();
        let mut iov = IoVec::empty();
        task_domain.copy_from_user(ptr, iov.as_bytes_mut())?;
        let base = iov.base;
        if base == 0 || iov.len == 0 {
            continue;
        }
        let len = iov.len;
        let mut tmp_buf = DVec::<u8>::new_uninit(len);
        let r;
        (tmp_buf, r) = vfs.vfs_read(file, tmp_buf)?;
        task_domain.copy_to_user(base, &tmp_buf.as_slice()[..r])?;
        count += r;
    }
    Ok(count as isize)
}

pub fn sys_writev(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    iov: usize,
    iovcnt: usize,
) -> AlienResult<isize> {
    info!(
        "<sys_writev> fd: {:?} iov: {:#x} iovcnt: {:?}",
        fd, iov, iovcnt
    );
    let file = task_domain.get_fd(fd)?;
    let mut count = 0;
    for i in 0..iovcnt {
        let ptr = iov + i * core::mem::size_of::<IoVec>();
        let mut iov = IoVec::empty();
        task_domain.copy_from_user(ptr, iov.as_bytes_mut())?;
        let base = iov.base;
        if base == 0 || iov.len == 0 {
            continue;
        }
        let len = iov.len;
        let mut tmp_buf = DVec::<u8>::new_uninit(len);
        task_domain.copy_from_user(base, tmp_buf.as_mut_slice())?;
        let w = vfs.vfs_write(file, &tmp_buf, len)?;
        count += w;
    }
    Ok(count as isize)
}

pub fn sys_fstatat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    dirfd: usize,
    path_ptr: *const u8,
    statbuf: usize,
    flags: usize,
) -> AlienResult<isize> {
    if path_ptr.is_null() {
        return Err(AlienError::EINVAL);
    }
    let mut tmp_buf = DVec::<u8>::new_uninit(256);
    let len;
    (tmp_buf, len) = task_domain.read_string_from_user(path_ptr as usize, tmp_buf)?;
    let path = core::str::from_utf8(&tmp_buf.as_slice()[..len]).unwrap();
    let flag = StatFlags::from_bits_truncate(flags as u32);
    info!(
        "<sys_fstatat> path_ptr: {:#x?}, path: {:?}, len:{} flags: {:?}",
        path_ptr, path, len, flag
    );
    let (_, current_root) = user_path_at(task_domain, dirfd as isize, path)?;
    // todo!(VfsFileStat == FileStat)
    let attr = DBox::<VfsFileStat>::new_uninit();
    let file = vfs.vfs_open(current_root, &tmp_buf, len, 0, 0)?;
    let stat = vfs.vfs_getattr(file, attr)?;
    let file_stat = FileStat::from(*stat);
    debug!("<sys_fstatat> file_stat: {:?}", file_stat);
    task_domain.copy_to_user(statbuf, file_stat.as_bytes())?;
    vfs.vfs_close(file)?;
    Ok(0)
}

pub fn sys_ftruncate(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    len: usize,
) -> AlienResult<isize> {
    let file = task_domain.get_fd(fd)?;
    vfs.vfs_ftruncate(file, len as u64)?;
    Ok(0)
}

pub fn sys_faccessat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    dirfd: usize,
    path: usize,
    mode: usize,
    flag: usize,
) -> AlienResult<isize> {
    if path == 0 {
        return Err(AlienError::EINVAL);
    }
    let mode = FaccessatMode::from_bits_truncate(mode as u32);
    let flag = FaccessatFlags::from_bits_truncate(flag as u32);
    let mut tmp_buf = DVec::<u8>::new_uninit(256);
    let len;
    (tmp_buf, len) = task_domain.read_string_from_user(path, tmp_buf)?;
    let path = core::str::from_utf8(&tmp_buf.as_slice()[..len]).unwrap();
    info!(
        "<sys_faccessat> path: {:?} flag: {:?} mode: {:?}",
        path, flag, mode
    );
    let (_, current_root) = user_path_at(task_domain, dirfd as isize, path)?;
    let id = vfs.vfs_open(current_root, &tmp_buf, len, 0, 0)?;
    info!("<sys_faccessat> id: {:?}", id);
    vfs.vfs_close(id)?;
    Ok(0)
}

pub fn sys_lseek(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    offset: usize,
    whence: usize,
) -> AlienResult<isize> {
    let file = task_domain.get_fd(fd)?;
    let seek = SeekFrom::try_from((whence, offset)).map_err(|_| AlienError::EINVAL)?;
    let res = vfs.vfs_lseek(file, seek)?;
    Ok(res as isize)
}

pub fn sys_fstat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    statbuf: usize,
) -> AlienResult<isize> {
    if statbuf == 0 {
        return Err(AlienError::EINVAL);
    }
    let file = task_domain.get_fd(fd)?;
    let attr = DBox::<VfsFileStat>::new_uninit();
    let stat = vfs.vfs_getattr(file, attr)?;
    let file_stat = FileStat::from(*stat);
    task_domain.copy_to_user(statbuf, file_stat.as_bytes())?;
    Ok(0)
}

pub fn sys_fsync(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
) -> AlienResult<isize> {
    let file = task_domain.get_fd(fd)?;
    vfs.vfs_fsync(file)?;
    Ok(0)
}

pub fn sys_utimensat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    dirfd: usize,
    path_ptr: usize,
    times_ptr: usize,
    _flags: usize,
) -> AlienResult<isize> {
    const UTIME_NOW: usize = 0x3fffffff;
    const UTIME_OMIT: usize = 0x3ffffffe;
    info!(
        "<utimensat> dirfd: {:?} path_ptr: {:#x} times_ptr: {:#x}",
        dirfd as isize, path_ptr, times_ptr
    );
    let tmp_buf = DVec::<u8>::new_uninit(256);
    let (tmp_buf, len) = task_domain.read_string_from_user(path_ptr, tmp_buf)?;
    let path = core::str::from_utf8(&tmp_buf.as_slice()[..len]).unwrap();
    info!("<utimensat>: path: {:?}", path);
    let (_, current_root) = user_path_at(task_domain, dirfd as isize, path)?;
    let file_inode = vfs.vfs_open(current_root, &tmp_buf, len, 0, 0)?;

    info!("<utimensat> inode id: {:?}", file_inode);
    if times_ptr == 0 {
        let time_now = TimeSpec::now();
        info!("set all time to time_now: {:?}", time_now);
        vfs.vfs_update_atime(file_inode, time_now.tv_sec as u64, time_now.tv_nsec as u64)?;
        vfs.vfs_update_mtime(file_inode, time_now.tv_sec as u64, time_now.tv_nsec as u64)?;
    } else {
        let atime = task_domain.read_val_from_user::<TimeSpec>(times_ptr)?;
        let mtime = task_domain
            .read_val_from_user::<TimeSpec>(times_ptr + core::mem::size_of::<TimeSpec>())?;
        info!("set atime: {:?}, mtime: {:?}", atime, mtime);
        let now = TimeSpec::now();
        if atime.tv_nsec == UTIME_NOW {
            vfs.vfs_update_atime(file_inode, now.tv_sec as u64, now.tv_nsec as u64)?;
        } else if atime.tv_nsec == UTIME_OMIT {
            // do nothing
        } else {
            vfs.vfs_update_atime(file_inode, atime.tv_sec as u64, atime.tv_nsec as u64)?;
        };
        if mtime.tv_nsec == UTIME_NOW {
            vfs.vfs_update_mtime(file_inode, now.tv_sec as u64, now.tv_nsec as u64)?;
        } else if mtime.tv_nsec == UTIME_OMIT {
            // do nothing
        } else {
            vfs.vfs_update_mtime(file_inode, mtime.tv_sec as u64, mtime.tv_nsec as u64)?;
        };
    }
    Ok(0)
}

pub fn sys_sendfile(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    out_fd: usize,
    in_fd: usize,
    offset_ptr: usize,
    mut count: usize,
) -> AlienResult<isize> {
    let in_file = task_domain.get_fd(in_fd)?;
    let out_file = task_domain.get_fd(out_fd)?;
    const MAX_COUNT: usize = 0x7fff_f000;
    if count > MAX_COUNT {
        count = MAX_COUNT;
    }
    let mut shared_buf = DVec::new_uninit(512);
    let mut total = 0;

    let mut offset = if offset_ptr != 0 {
        let offset = task_domain.read_val_from_user::<u64>(offset_ptr)?;
        Some(offset)
    } else {
        None
    };
    while total < count {
        let (buf, r) = if let Some(offset) = offset.as_mut() {
            let (buf, r) = vfs.vfs_read_at(in_file, *offset, shared_buf)?;
            *offset += r as u64;
            (buf, r)
        } else {
            let (buf, r) = vfs.vfs_read(in_file, shared_buf)?;
            (buf, r)
        };
        if r == 0 {
            break;
        }
        total += r;
        let w = vfs.vfs_write(out_file, &buf, r)?;
        if w != r {
            break;
        }
        shared_buf = buf;
    }
    debug!("sendfile: write {} bytes,arg count: {}", total, count);
    if let Some(offset) = offset {
        task_domain.write_val_to_user(offset_ptr, &offset)?;
    }
    Ok(total as isize)
}

pub struct SelectArgs {
    pub nfds: usize,
    pub readfds: usize,
    pub writefds: usize,
    pub exceptfds: usize,
    pub timeout: usize,
    pub sigmask: usize,
}

pub fn sys_pselect6(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    args: SelectArgs,
) -> AlienResult<isize> {
    let SelectArgs {
        nfds,
        readfds,
        writefds,
        exceptfds,
        timeout,
        sigmask,
    } = args;
    debug!(
        "<sys_pselect6> nfds: {:?} readfds: {:?} writefds: {:?} exceptfds: {:?} timeout: {:?} sigmask: {:?}",
        nfds, readfds, writefds, exceptfds, timeout, sigmask
    );
    if nfds >= MAX_FD_NUM {
        return Err(AlienError::EINVAL);
    }
    let (wait_time, time_spec) = if timeout != 0 {
        let time_spec = task_domain.read_val_from_user::<TimeSpec>(timeout)?;
        debug!("pselect6: timeout = {:#x} ---> {:?}", timeout, time_spec);
        (
            Some(time_spec.to_clock() + TimeSpec::now().to_clock()),
            Some(time_spec),
        )
    } else {
        (Some(usize::MAX), None)
    };
    let nfds = min(nfds, 64);
    let ori_readfds = if readfds != 0 {
        task_domain.read_val_from_user::<u64>(readfds)?
    } else {
        0
    };

    let ori_writefds = if writefds != 0 {
        task_domain.read_val_from_user::<u64>(writefds)?
    } else {
        0
    };

    let ori_exceptfds = if exceptfds != 0 {
        task_domain.read_val_from_user::<u64>(exceptfds)?
    } else {
        0
    };

    loop {
        let mut set = 0;
        if readfds != 0 {
            let mut readfds_mask = ori_readfds;
            for i in 0..nfds {
                if ori_readfds.get_bit(i) {
                    let inode_id = task_domain.get_fd(i)?;
                    let event = vfs
                        .vfs_poll(inode_id, VfsPollEvents::IN)
                        .expect("poll error");
                    if event.contains(VfsPollEvents::IN) {
                        debug!("pselect6: fd {} ready to read", i);
                        readfds_mask.set_bit(i, true);
                        set += 1;
                    } else {
                        readfds_mask.set_bit(i, false);
                    }
                }
            }
            task_domain.write_val_to_user(readfds, &readfds_mask)?;
        }
        if writefds != 0 {
            let mut writefds_mask = ori_writefds;
            for i in 0..nfds {
                if ori_writefds.get_bit(i) {
                    let inode_id = task_domain.get_fd(i)?;
                    let event = vfs
                        .vfs_poll(inode_id, VfsPollEvents::OUT)
                        .expect("poll error");
                    if event.contains(VfsPollEvents::OUT) {
                        debug!("pselect6: fd {} ready to write", i);
                        writefds_mask.set_bit(i, true);
                        set += 1;
                    } else {
                        writefds_mask.set_bit(i, false);
                    }
                }
            }
            task_domain.write_val_to_user(writefds, &writefds_mask)?;
        }
        if exceptfds != 0 {
            let mut exceptfds_mask = ori_exceptfds;
            for i in 0..nfds {
                if ori_exceptfds.get_bit(i) {
                    let inode_id = task_domain.get_fd(i)?;
                    let event = vfs
                        .vfs_poll(inode_id, VfsPollEvents::ERR)
                        .expect("poll error");
                    if event.contains(VfsPollEvents::ERR) {
                        debug!("pselect6: fd {} ready to except", i);
                        exceptfds_mask.set_bit(i, true);
                        set += 1;
                    } else {
                        exceptfds_mask.set_bit(i, false);
                    }
                }
            }
            task_domain.write_val_to_user(exceptfds, &exceptfds_mask)?;
        }
        if set > 0 {
            return Ok(set as isize);
        }

        if let Some(time_spec) = time_spec {
            if time_spec == TimeSpec::new(0, 0) {
                return Ok(0);
            }
        }

        basic::yield_now()?;

        if let Some(wait_time) = wait_time {
            if wait_time <= TimeSpec::now().to_clock() {
                debug!(
                    "select timeout, wait_time = {:#x}, now = {:#x}",
                    wait_time,
                    TimeSpec::now().to_clock()
                );
                return Ok(0);
            }
        }
    }
}

pub fn sys_ppoll(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fds_ptr: usize,
    nfds: usize,
    timeout: usize,
    sigmask: usize,
) -> AlienResult<isize> {
    debug!(
        "<sys_ppoll> fds: {:#x} nfds: {:?} timeout: {:#x} sigmask: {:#x}",
        fds_ptr, nfds, timeout, sigmask
    );
    let mut fds = vec![0u8; core::mem::size_of::<PollFd>() * nfds];
    task_domain.copy_from_user(fds_ptr, fds.as_mut_slice())?;
    debug!("fds: {:?}", fds);
    let wait_time = if sigmask == usize::MAX {
        if timeout == usize::MAX {
            None
        } else {
            let sec = timeout / 1000;
            let nsec = (timeout % 1000) * 1_000_000;
            let time_spec = TimeSpec::new(sec, nsec);
            Some(time_spec.to_clock() + TimeSpec::now().to_clock())
        }
    } else if timeout != 0 {
        let time_spec = task_domain.read_val_from_user::<TimeSpec>(timeout)?;
        Some(time_spec.to_clock() + TimeSpec::now().to_clock())
    } else {
        None
    }; // wait forever
    let mut res = 0;
    loop {
        for idx in 0..nfds {
            let mut pfd = PollFd::from_bytes(&fds[idx * core::mem::size_of::<PollFd>()..]);
            if let Ok(file) = task_domain.get_fd(pfd.fd as usize) {
                let vfs_event = VfsPollEvents::from_bits_truncate(pfd.events.bits() as u16);
                let event = vfs.vfs_poll(file, vfs_event)?;
                if !event.is_empty() {
                    res += 1;
                }
                debug!("[ppoll]: event: {:?}", event);
                pfd.revents = PollEvents::from_bits_truncate(event.bits() as u32)
            } else {
                // todo: error
                pfd.events = PollEvents::EPOLLERR;
            }
            let range = (idx * core::mem::size_of::<PollFd>())
                ..((idx + 1) * core::mem::size_of::<PollFd>());
            fds[range].copy_from_slice(pfd.as_bytes());
        }
        if res > 0 {
            // copy to user
            task_domain.copy_to_user(fds_ptr, &fds)?;
            debug!("ppoll return {:?}", fds);
            return Ok(res as isize);
        }
        if let Some(wait_time) = wait_time {
            if wait_time <= TimeSpec::now().to_clock() {
                debug!("ppoll timeout");
                return Ok(0);
            }
        }
        debug!("<sys_ppoll> suspend");
        basic::yield_now()?;
    }
}

pub fn sys_getdents64(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    buf: usize,
    count: usize,
) -> AlienResult<isize> {
    let file = task_domain.get_fd(fd)?;
    let mut tmp_buf = DVec::<u8>::new_uninit(count);
    let r;
    (tmp_buf, r) = vfs.vfs_readdir(file, tmp_buf)?;
    info!(
        "<sys_getdents64> fd: {:?} buf: {:#x} count: {:?} r: {:?}",
        fd, buf, count, r
    );
    task_domain.copy_to_user(buf, &tmp_buf.as_slice()[..r])?;
    Ok(r as isize)
}

pub fn sys_chdir(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    path: usize,
) -> AlienResult<isize> {
    let mut tmp_buf = DVec::<u8>::new_uninit(128);
    let len;
    (tmp_buf, len) = task_domain.read_string_from_user(path, tmp_buf)?;
    let path = core::str::from_utf8(&tmp_buf.as_slice()[..len]).unwrap();
    // basic::println_color!(31,"<sys_chdir> path: {:?}", path);
    let (_, current_root) = user_path_at(task_domain, AT_FDCWD, path)?;
    let id = vfs.vfs_open(current_root, &tmp_buf, len, 0, 0)?;
    task_domain.set_cwd(id)?;
    Ok(0)
}

pub fn sys_getcwd(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    buf: usize,
    size: usize,
) -> AlienResult<isize> {
    if buf == 0 {
        return Err(AlienError::EINVAL);
    }
    let (_, cwd) = task_domain.fs_info()?;
    let mut tmp_buf = DVec::<u8>::new(0, 128);
    let r;
    (tmp_buf, r) = vfs.vfs_get_path(cwd, tmp_buf)?;
    // let cwd = core::str::from_utf8(&tmp_buf.as_slice()[..r]).unwrap();
    info!("<sys_getcwd> buf: {:#x} size: {:?} r: {:?}", buf, size, r);
    // basic::println_color!(31,"<sys_getcwd> cwd: {:?}", cwd);
    if r + 1 > size {
        return Err(AlienError::ERANGE);
    }
    task_domain.copy_to_user(buf, &tmp_buf.as_slice()[..r + 1])?;
    Ok(buf as isize)
}

pub fn sys_mkdirat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    dirfd: usize,
    path_ptr: usize,
    mode: usize,
) -> AlienResult<isize> {
    let tmp_buf = DVec::<u8>::new_uninit(256);
    let (tmp_buf, len) = task_domain.read_string_from_user(path_ptr, tmp_buf)?;
    let mut mode = InodeMode::from_bits_truncate(mode as u32);
    let path = core::str::from_utf8(&tmp_buf.as_slice()[..len]).unwrap();
    mode |= InodeMode::DIR;
    info!("<sys_mkdirat> path: {:?},  mode: {:?}", path, mode);
    let (_, current_root) = user_path_at(task_domain, dirfd as isize, path)?;
    let _id = vfs.vfs_open(
        current_root,
        &tmp_buf,
        len,
        mode.bits(),
        OpenFlags::O_CREAT.bits(),
    )?;
    Ok(0)
}

pub fn sys_unlinkat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    dirfd: usize,
    path_ptr: usize,
    flags: usize,
) -> AlienResult<isize> {
    let tmp_buf = DVec::<u8>::new_uninit(256);
    let (tmp_buf, len) = task_domain.read_string_from_user(path_ptr, tmp_buf)?;
    let path = core::str::from_utf8(&tmp_buf.as_slice()[..len]).unwrap();
    let flag = UnlinkatFlags::from_bits_truncate(flags as u32);
    info!("<sys_unlinkat> path: {:?}, flags: {:?}", path, flag);
    let (_, current_root) = user_path_at(task_domain, dirfd as isize, path)?;
    vfs.vfs_unlink(current_root, &tmp_buf, len, flag.bits())?;
    Ok(0)
}

pub fn sys_renameat2(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    olddirfd: usize,
    oldpath: usize,
    newdirfd: usize,
    newpath: usize,
    flags: usize,
) -> AlienResult<isize> {
    let old_tmp_buf = DVec::<u8>::new_uninit(256);
    let new_tmp_buf = DVec::<u8>::new_uninit(256);
    let (old_tmp_buf, old_len) = task_domain.read_string_from_user(oldpath, old_tmp_buf)?;
    let (new_tmp_buf, new_len) = task_domain.read_string_from_user(newpath, new_tmp_buf)?;
    let old_path = core::str::from_utf8(&old_tmp_buf.as_slice()[..old_len]).unwrap();
    let new_path = core::str::from_utf8(&new_tmp_buf.as_slice()[..new_len]).unwrap();
    let flag = Renameat2Flags::from_bits_truncate(flags as u32);
    log::info!(
        "<sys_renameat2> olddirfd: {} oldpath: {:?} newdirfd: {} newpath: {:?} flags: {:?}",
        olddirfd as isize,
        old_path,
        newdirfd as isize,
        new_path,
        flag
    );
    if flag.contains(Renameat2Flags::RENAME_EXCHANGE)
        && (flag.contains(Renameat2Flags::RENAME_NOREPLACE)
            || flag.contains(Renameat2Flags::RENAME_WHITEOUT))
    {
        return Err(LinuxErrno::EINVAL);
    }
    let (cwd, root) = task_domain.fs_info()?;

    let (_, old_root) = user_path_at(task_domain, olddirfd as isize, old_path)?;
    let (_, new_root) = user_path_at(task_domain, newdirfd as isize, new_path)?;
    let res = vfs.vfs_rename(
        old_root,
        new_root,
        &old_tmp_buf,
        old_len,
        &new_tmp_buf,
        new_len,
        (cwd, root),
        flags as u32,
    );
    log::info!("<sys_renameat2> res: {:?}", res);
    res?;
    Ok(0)
}

pub fn sys_truncate(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    path_ptr: usize,
    len: usize,
) -> AlienResult<isize> {
    if path_ptr == 0 {
        return Err(AlienError::EFAULT);
    }
    let mut tmp_buf = DVec::<u8>::new_uninit(256);
    let path_len;
    (tmp_buf, path_len) = task_domain.read_string_from_user(path_ptr, tmp_buf)?;
    let path = core::str::from_utf8(&tmp_buf.as_slice()[..path_len]).unwrap();
    info!("<sys_truncate> path: {:?}, len: {}", path, len);
    let (_, current_root) = user_path_at(task_domain, AT_FDCWD, path)?;
    let inode = vfs.vfs_open(current_root, &tmp_buf, path_len, 0, OpenFlags::O_RDWR.bits())?;
    let res = vfs.vfs_ftruncate(inode, len as u64);
    let _ = vfs.vfs_close(inode);
    res?;
    Ok(0)
}

pub fn sys_statfs(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    path: usize,
    statbuf: usize,
) -> AlienResult<isize> {
    if path == 0 || statbuf == 0 {
        return Err(AlienError::EFAULT);
    }

    let tmp_buf = DVec::<u8>::new_uninit(256);
    let (tmp_buf, len) = task_domain.read_string_from_user(path, tmp_buf)?;
    let path_str = core::str::from_utf8(&tmp_buf.as_slice()[..len]).unwrap();
    info!("<sys_statfs> path: {:?}", path_str);

    let (_, current_root) = user_path_at(task_domain, AT_FDCWD, path_str)?;
    let inode = vfs.vfs_open(current_root, &tmp_buf, len, 0, OpenFlags::O_RDONLY.bits())?;
    let vfs_stat = vfs.vfs_statfs(inode, DBox::<VfsFsStat>::new_uninit());
    let _ = vfs.vfs_close(inode);
    let vfs_stat = vfs_stat?;

    let fs_stat = map_vfs_fs_stat(*vfs_stat);
    task_domain.write_val_to_user(statbuf, &fs_stat)?;
    Ok(0)
}

pub fn sys_fstatfs(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    fd: usize,
    statbuf: usize,
) -> AlienResult<isize> {
    if statbuf == 0 {
        return Err(AlienError::EFAULT);
    }
    let inode = task_domain.get_fd(fd)?;
    let vfs_stat = vfs.vfs_statfs(inode, DBox::<VfsFsStat>::new_uninit())?;
    let fs_stat = map_vfs_fs_stat(*vfs_stat);
    task_domain.write_val_to_user(statbuf, &fs_stat)?;
    Ok(0)
}

fn map_vfs_fs_stat(vfs_stat: VfsFsStat) -> FsStat {
    FsStat {
        f_type: vfs_stat.f_type,
        f_bsize: vfs_stat.f_bsize,
        f_blocks: vfs_stat.f_blocks,
        f_bfree: vfs_stat.f_bfree,
        f_bavail: vfs_stat.f_bavail,
        f_files: vfs_stat.f_files,
        f_ffree: vfs_stat.f_ffree,
        f_fsid: vfs_stat.f_fsid,
        f_namelen: vfs_stat.f_namelen,
        f_frsize: vfs_stat.f_frsize,
        f_flags: vfs_stat.f_flags,
        f_spare: vfs_stat.f_spare,
    }
}

pub fn sys_mount(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _source: usize,
    _target: usize,
    _fs_type: usize,
    _flags: usize,
    _data: usize,
) -> AlienResult<isize> {
    todo!("mount 暂未实现")
}

pub fn sys_linkat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    olddirfd: usize,
    oldpath: usize,
    newdirfd: usize,
    newpath: usize,
    flags: usize,
) -> AlienResult<isize> {
    if oldpath == 0 || newpath == 0 {
        return Err(AlienError::EFAULT);
    }

    let old_tmp_buf = DVec::<u8>::new_uninit(256);
    let new_tmp_buf = DVec::<u8>::new_uninit(256);
    let (old_tmp_buf, old_len) = task_domain.read_string_from_user(oldpath, old_tmp_buf)?;
    let (new_tmp_buf, new_len) = task_domain.read_string_from_user(newpath, new_tmp_buf)?;
    let old_path = core::str::from_utf8(&old_tmp_buf.as_slice()[..old_len]).unwrap();
    let new_path = core::str::from_utf8(&new_tmp_buf.as_slice()[..new_len]).unwrap();

    let link_flags = LinkFlags::from_bits(flags as u32).ok_or(AlienError::EINVAL)?;
    if link_flags.contains(LinkFlags::AT_EMPTY_PATH) {
        return Err(AlienError::ENOSYS);
    }

    info!(
        "<sys_linkat> olddirfd: {}, oldpath: {:?}, newdirfd: {}, newpath: {:?}, flags: {:?}",
        olddirfd as isize,
        old_path,
        newdirfd as isize,
        new_path,
        link_flags
    );

    let (_, old_root) = user_path_at(task_domain, olddirfd as isize, old_path)?;
    let (_, new_root) = user_path_at(task_domain, newdirfd as isize, new_path)?;
    vfs.vfs_linkat(
        old_root,
        &old_tmp_buf,
        old_len,
        new_root,
        &new_tmp_buf,
        new_len,
        link_flags.bits(),
    )?;
    Ok(0)
}

pub fn sys_symlinkat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    oldpath: usize,
    newdirfd: usize,
    newpath: usize,
) -> AlienResult<isize> {
    if oldpath == 0 || newpath == 0 {
        return Err(AlienError::EFAULT);
    }

    let old_tmp_buf = DVec::<u8>::new_uninit(256);
    let new_tmp_buf = DVec::<u8>::new_uninit(256);
    let (old_tmp_buf, old_len) = task_domain.read_string_from_user(oldpath, old_tmp_buf)?;
    let (new_tmp_buf, new_len) = task_domain.read_string_from_user(newpath, new_tmp_buf)?;
    let target = core::str::from_utf8(&old_tmp_buf.as_slice()[..old_len]).unwrap();
    let new_path = core::str::from_utf8(&new_tmp_buf.as_slice()[..new_len]).unwrap();

    info!(
        "<sys_symlinkat> target: {:?}, newdirfd: {}, newpath: {:?}",
        target,
        newdirfd as isize,
        new_path
    );

    let (_, new_root) = user_path_at(task_domain, newdirfd as isize, new_path)?;
    vfs.vfs_symlinkat(&old_tmp_buf, old_len, new_root, &new_tmp_buf, new_len)?;
    Ok(0)
}

pub fn sys_readlinkat(
    vfs: &Arc<dyn VfsDomain>,
    task_domain: &Arc<dyn TaskDomain>,
    dirfd: usize,
    path: usize,
    buf: usize,
    bufsiz: usize,
) -> AlienResult<isize> {
    if path == 0 || (buf == 0 && bufsiz != 0) {
        return Err(AlienError::EFAULT);
    }
    if bufsiz == 0 {
        return Ok(0);
    }

    let tmp_path = DVec::<u8>::new_uninit(256);
    let (tmp_path, path_len) = task_domain.read_string_from_user(path, tmp_path)?;
    let path = core::str::from_utf8(&tmp_path.as_slice()[..path_len]).unwrap();
    info!(
        "<sys_readlinkat> dirfd: {}, path: {:?}, buf: {:#x}, bufsiz: {}",
        dirfd as isize,
        path,
        buf,
        bufsiz
    );

    let (_, root) = user_path_at(task_domain, dirfd as isize, path)?;
    let mut out_buf = DVec::<u8>::new_uninit(bufsiz);
    let read_len;
    (out_buf, read_len) = vfs.vfs_readlinkat(root, &tmp_path, path_len, out_buf)?;

    let copy_len = min(read_len, bufsiz);
    task_domain.copy_to_user(buf, &out_buf.as_slice()[..copy_len])?;
    Ok(copy_len as isize)
}

pub fn sys_setxattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _path: usize,
    _name: usize,
    _value: usize,
    _size: usize,
    _flags: usize,
) -> AlienResult<isize> {
    todo!("setxattr 暂未实现")
}

pub fn sys_lsetxattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _path: usize,
    _name: usize,
    _value: usize,
    _size: usize,
    _flags: usize,
) -> AlienResult<isize> {
    todo!("lsetxattr 暂未实现")
}

pub fn sys_fsetxattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _fd: usize,
    _name: usize,
    _value: usize,
    _size: usize,
    _flags: usize,
) -> AlienResult<isize> {
    todo!("fsetxattr 暂未实现")
}

pub fn sys_getxattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _path: usize,
    _name: usize,
    _value: usize,
    _size: usize,
) -> AlienResult<isize> {
    todo!("getxattr 暂未实现")
}

pub fn sys_lgetxattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _path: usize,
    _name: usize,
    _value: usize,
    _size: usize,
) -> AlienResult<isize> {
    todo!("lgetxattr 暂未实现")
}

pub fn sys_fgetxattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _fd: usize,
    _name: usize,
    _value: usize,
    _size: usize,
) -> AlienResult<isize> {
    todo!("fgetxattr 暂未实现")
}

pub fn sys_listxattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _path: usize,
    _list: usize,
    _size: usize,
) -> AlienResult<isize> {
    todo!("listxattr 暂未实现")
}

pub fn sys_llistxattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _path: usize,
    _list: usize,
    _size: usize,
) -> AlienResult<isize> {
    todo!("llistxattr 暂未实现")
}

pub fn sys_flistxattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _fd: usize,
    _list: usize,
    _size: usize,
) -> AlienResult<isize> {
    todo!("flistxattr 暂未实现")
}

pub fn sys_removexattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _path: usize,
    _name: usize,
) -> AlienResult<isize> {
    todo!("removexattr 暂未实现")
}

pub fn sys_lremovexattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _path: usize,
    _name: usize,
) -> AlienResult<isize> {
    todo!("lremovexattr 暂未实现")
}

pub fn sys_fremovexattr(
    _vfs: &Arc<dyn VfsDomain>,
    _task_domain: &Arc<dyn TaskDomain>,
    _fd: usize,
    _name: usize,
) -> AlienResult<isize> {
    todo!("fremovexattr 暂未实现")
}
