#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::boxed::Box;
use core::fmt::Debug;

use basic::{
    sync::{Mutex, Once, OnceGet},
    AlienError, AlienResult,
};
use interface::{define_unwind_for_InputDomain, Basic, DeviceBase, InputDomain, VirtioInitInfo};
use virtio_drivers::device::input::VirtIOInput;
use virtio_drivers::transport::DeviceType;
use virtio_common::{transport_from_init_info, HalImpl, VirtioTransport};

#[derive(Default)]
pub struct InputDevDomain {
    input: Once<Mutex<VirtIOInput<HalImpl, VirtioTransport>>>,
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
    fn init(&self, init_info: &VirtioInitInfo) -> AlienResult<()> {
        let transport = transport_from_init_info(init_info, DeviceType::Input, "virtio_input");
        let input =
            VirtIOInput::<HalImpl, VirtioTransport>::new(transport).expect("failed to create input driver");
        self.input.call_once(|| Mutex::new(input));
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
