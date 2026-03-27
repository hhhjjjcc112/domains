#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::boxed::Box;
use core::{fmt::Debug, ops::Range};

use basic::{
    io::SafeIORegion,
    println,
    sync::{Mutex, Once, OnceGet},
    AlienError, AlienResult,
};
use interface::{define_unwind_for_InputDomain, Basic, DeviceBase, InputDomain};
#[cfg(target_arch = "riscv64")]
use virtio_drivers::transport::mmio::MmioTransport;
#[cfg(target_arch = "x86_64")]
use virtio_drivers::transport::{
    pci::{bus::{Cam, Command, PciRoot}, virtio_device_type, PciTransport},
    DeviceType,
};
use virtio_drivers::device::input::VirtIOInput;
use virtio_mmio_common::{HalImpl, SafeIORW};

#[derive(Default)]
pub struct InputDevDomain {
    #[cfg(target_arch = "riscv64")]
    input: Once<Mutex<VirtIOInput<HalImpl, MmioTransport>>>,
    #[cfg(target_arch = "x86_64")]
    input: Once<Mutex<VirtIOInput<HalImpl, PciTransport>>>,
}

impl Debug for InputDevDomain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("InputDevDomain")
    }
}

impl Basic for InputDevDomain {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}
impl DeviceBase for InputDevDomain {
    fn handle_irq(&self) -> AlienResult<()> {
        self.input.get_must().lock().ack_interrupt().unwrap();
        Ok(())
    }
}

impl InputDomain for InputDevDomain {
    fn init(&self, address_range: &Range<usize>) -> AlienResult<()> {
        #[cfg(target_arch = "riscv64")]
        {
            let io_region = SafeIORW(SafeIORegion::from(address_range.clone()));
            let transport = MmioTransport::new(Box::new(io_region)).unwrap();
            let input = VirtIOInput::<HalImpl, MmioTransport>::new(transport)
                .expect("failed to create input driver");
            self.input.call_once(|| Mutex::new(input));
            return Ok(());
        }

        #[cfg(target_arch = "x86_64")]
        {
            let io_region = SafeIORW(SafeIORegion::from(address_range.clone()));
            let mut root = PciRoot::new(Box::new(io_region), Cam::Ecam);

            let mut found = None;
            'outer: for bus in 0u8..=u8::MAX {
                for (device_function, info) in root.enumerate_bus(bus) {
                    if virtio_device_type(&info) == Some(DeviceType::Input) {
                        found = Some((device_function, info));
                        break 'outer;
                    }
                }
            }

            let (device_function, info) = found.expect("virtio-pci input not found in ECAM");
            println!("virtio-pci input bdf={} ({})", device_function, info);

            let (_status, mut command) = root.get_status_command(device_function);
            command.insert(Command::BUS_MASTER | Command::MEMORY_SPACE | Command::IO_SPACE);
            root.set_command(device_function, command);

            let transport = PciTransport::new::<HalImpl>(&mut root, device_function)
                .expect("failed to create virtio-pci transport");
            let input = VirtIOInput::<HalImpl, PciTransport>::new(transport)
                .expect("failed to create virtio-pci input driver");
            self.input.call_once(|| Mutex::new(input));
            return Ok(());
        }

        #[allow(unreachable_code)]
        Ok(())
    }

    fn event_nonblock(&self) -> AlienResult<Option<u64>> {
        match self.input.get_must().lock().pop_pending_event() {
            Ok(v) => {
                let val = v.map(|e| {
                    (e.event_type as u64) << 48 | (e.code as u64) << 32 | (e.value) as u64
                });
                Ok(val)
            }
            Err(_e) => Err(AlienError::EINVAL),
        }
    }
}
define_unwind_for_InputDomain!(InputDevDomain);

pub fn main() -> Box<dyn InputDomain> {
    Box::new(UnwindWrap::new(InputDevDomain::default()))
}
