#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::boxed::Box;
use core::{fmt::Debug, ops::Range};

use basic::{
    println,
    sync::{Mutex, Once, OnceGet},
    AlienError, AlienResult,
};
#[cfg(target_arch = "riscv64")]
use interface::define_unwind_for_UartDomain;
use interface::{Basic, DeviceBase, UartDomain};
use safe_uart_16550::{SafeUart16550, UartError};
use shared_heap::DVec;

#[derive(Default)]
struct UartDomainImpl {
    uart: Once<Mutex<SafeUart16550>>,
}

#[inline]
fn map_uart_error(err: UartError) -> AlienError {
    match err {
        UartError::InvalidAddressRange | UartError::UnsupportedTransport => AlienError::EINVAL,
        UartError::IoRegionAccessFailed => AlienError::EIO,
    }
}

impl Debug for UartDomainImpl {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("UartDomainImpl")
    }
}

impl DeviceBase for UartDomainImpl {
    fn handle_irq(&self) -> AlienResult<()> {
        todo!()
    }
}

impl Basic for UartDomainImpl {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl UartDomain for UartDomainImpl {
    fn init(&self, address_range: &Range<usize>) -> AlienResult<()> {
        let region = address_range;
        println!("uart_addr: {:#x}-{:#x}", region.start, region.end);
        // 域内保持纯 safe，unsafe 由 utils/safe_uart_16550 内部封装。
        #[cfg(target_arch = "x86_64")]
        let mut uart = SafeUart16550::new_pio(region).map_err(map_uart_error)?;
        #[cfg(target_arch = "riscv64")]
        let mut uart = SafeUart16550::new_mmio(region).map_err(map_uart_error)?;
        uart.init();
        self.uart.call_once(|| Mutex::new(uart));
        self.enable_receive_interrupt()?;
        println!("init uart success");
        Ok(())
    }

    fn putc(&self, ch: u8) -> AlienResult<()> {
        let mut uart = self.uart.get_must().lock();
        if ch == b'\n' {
            uart.putc(b'\r');
        }
        uart.putc(ch);
        Ok(())
    }

    fn getc(&self) -> AlienResult<Option<u8>> {
        Ok(self.uart.get_must().lock().getc_nonblocking())
    }

    fn put_bytes(&self, buf: &DVec<u8>) -> AlienResult<usize> {
        let mut uart = self.uart.get_must().lock();
        Ok(uart.put_bytes(buf.as_slice()))
    }

    fn have_data_to_get(&self) -> AlienResult<bool> {
        self.uart
            .get_must()
            .lock()
            .have_data_to_get()
            .map_err(map_uart_error)
    }

    fn enable_receive_interrupt(&self) -> AlienResult<()> {
        self.uart
            .get_must()
            .lock()
            .enable_receive_interrupt()
            .map_err(map_uart_error)
    }

    fn disable_receive_interrupt(&self) -> AlienResult<()> {
        self.uart
            .get_must()
            .lock()
            .disable_receive_interrupt()
            .map_err(map_uart_error)
    }
}

#[cfg(target_arch = "riscv64")]
define_unwind_for_UartDomain!(UartDomainImpl);
pub fn main() -> Box<dyn UartDomain> {
    #[cfg(target_arch = "riscv64")]
    {
        Box::new(UnwindWrap::new(UartDomainImpl::default()))
    }
    #[cfg(target_arch = "x86_64")]
    {
        Box::new(UartDomainImpl::default())
    }
}
