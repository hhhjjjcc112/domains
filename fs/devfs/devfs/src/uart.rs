use alloc::sync::Arc;

use basic::{
    constants::{
        io::{LocalModes, TeletypeCommand, Termios, WinSize},
        DeviceId,
    },
    sync::Mutex,
};
use interface::{BufUartDomain, TaskDomain};
use pod::Pod;
use shared_heap::DVec;
use vfscore::{
    error::VfsError,
    file::VfsFile,
    inode::{InodeAttr, VfsInode},
    superblock::VfsSuperBlock,
    utils::{VfsFileStat, VfsNodeType, VfsPollEvents},
    VfsResult,
};

#[derive(Debug, Default)]
pub struct IoData {
    foreground_pgid: u32,
    winsize: WinSize,
    termios: Termios,
}

pub struct UARTDevice {
    device_id: DeviceId,
    device: Arc<dyn BufUartDomain>,
    io: Mutex<IoData>,
    task_domain: Arc<dyn TaskDomain>,
}

impl UARTDevice {
    pub fn new(
        device_id: DeviceId,
        device: Arc<dyn BufUartDomain>,
        task: Arc<dyn TaskDomain>,
    ) -> Self {
        Self {
            device_id,
            device,
            io: Mutex::new(IoData::default()),
            task_domain: task,
        }
    }
}

impl VfsFile for UARTDevice {
    fn read_at(&self, _offset: u64, mut _buf: DVec<u8>) -> VfsResult<(DVec<u8>, usize)> {
        let buf = _buf.as_mut_slice();
        if buf.is_empty() {
            return Ok((_buf, 0));
        }

        let echo_enabled = LocalModes::from_bits_truncate(self.io.lock().termios.lflag)
            .contains(LocalModes::ECHO);
        let mut read_count = 0;
        loop {
            let ch = if read_count == 0 {
                self.device.getc().unwrap()
            } else {
                if !self.device.have_data_to_get().unwrap() {
                    break;
                }
                self.device.getc().unwrap()
            };

            let Some(ch) = ch else {
                if read_count > 0 {
                    break;
                }
                continue;
            };

            let is_newline = ch == b'\r' || ch == b'\n';
            let out = if ch == b'\r' { b'\n' } else { ch };

            buf[read_count] = out;
            read_count += 1;

            if echo_enabled {
                self.device.putc(out).unwrap();
            }

            if is_newline {
                break;
            }

            if read_count >= buf.len() {
                break;
            }
        }
        Ok((_buf, read_count))
    }
    fn write_at(&self, _offset: u64, buf: &DVec<u8>) -> VfsResult<usize> {
        self.device.put_bytes(buf).unwrap();
        Ok(buf.len())
    }
    fn poll(&self, event: VfsPollEvents) -> VfsResult<VfsPollEvents> {
        let mut res = VfsPollEvents::empty();
        if event.contains(VfsPollEvents::IN) && self.device.have_data_to_get().unwrap() {
            res |= VfsPollEvents::IN;
        }
        if event.contains(VfsPollEvents::OUT) && self.device.have_space_to_put().unwrap() {
            res |= VfsPollEvents::OUT
        }
        Ok(res)
    }
    fn ioctl(&self, cmd: u32, arg: usize) -> VfsResult<usize> {
        let mut io = self.io.lock();
        let cmd = TeletypeCommand::try_from(cmd).unwrap();
        match cmd {
            TeletypeCommand::TCGETS | TeletypeCommand::TCGETA => {
                self.task_domain
                    .copy_to_user(arg, io.termios.as_bytes())
                    .unwrap();
                Ok(0)
            }
            TeletypeCommand::TCSETS | TeletypeCommand::TCSETSW | TeletypeCommand::TCSETSF => {
                let buf = io.termios.as_bytes_mut();
                self.task_domain.copy_from_user(arg, buf).unwrap();
                Ok(0)
            }
            TeletypeCommand::TIOCGPGRP => {
                self.task_domain
                    .write_val_to_user(arg, &io.foreground_pgid)
                    .unwrap();
                Ok(0)
            }
            TeletypeCommand::TIOCSPGRP => {
                let word = self.task_domain.read_val_from_user(arg).unwrap();
                io.foreground_pgid = word;
                Ok(0)
            }
            TeletypeCommand::TIOCGWINSZ => {
                self.task_domain
                    .copy_to_user(arg, io.winsize.as_bytes())
                    .unwrap();
                Ok(0)
            }
            TeletypeCommand::TIOCSWINSZ => {
                self.task_domain
                    .copy_from_user(arg, io.winsize.as_bytes_mut())
                    .unwrap();
                Ok(0)
            }
            _ => {
                unimplemented!("ioctl cmd: {:?}", cmd)
            }
        }
    }
    fn flush(&self) -> VfsResult<()> {
        Ok(())
    }
    fn fsync(&self) -> VfsResult<()> {
        Ok(())
    }
}

impl VfsInode for UARTDevice {
    fn get_super_block(&self) -> VfsResult<Arc<dyn VfsSuperBlock>> {
        Err(VfsError::NoSys)
    }

    fn set_attr(&self, _attr: InodeAttr) -> VfsResult<()> {
        Ok(())
    }

    fn get_attr(&self) -> VfsResult<VfsFileStat> {
        Ok(VfsFileStat {
            st_rdev: self.device_id.id(),
            ..Default::default()
        })
    }

    fn inode_type(&self) -> VfsNodeType {
        VfsNodeType::CharDevice
    }
}
