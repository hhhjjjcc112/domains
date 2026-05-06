#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use core::{
    fmt::{Debug, Formatter, Result},
};

use basic::{
    sync::{Mutex, Once, OnceGet},
    AlienResult,
};
use interface::{define_unwind_for_NetDeviceDomain, Basic, DeviceBase, NetDeviceDomain, VirtioInitInfo};
use shared_heap::DVec;
use virtio_drivers::device::net::VirtIONet;
use virtio_drivers::transport::DeviceType;
use virtio_common::{to_alien_err, transport_from_init_info, HalImpl, VirtioTransport};

pub const NET_QUEUE_SIZE: usize = 128;
pub const NET_BUF_LEN: usize = 4096;

#[derive(Default)]
pub struct VirtIoNetDomain {
    nic: Once<Mutex<VirtIONet<HalImpl, VirtioTransport, NET_QUEUE_SIZE>>>,
}

impl Debug for VirtIoNetDomain {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        writeln!(f, "NicDomain")
    }
}

impl Basic for VirtIoNetDomain {
    fn domain_id(&self) -> u64 {
        shared_heap::domain_id()
    }
}

impl DeviceBase for VirtIoNetDomain {
    fn handle_irq(&self) -> AlienResult<()> {
        log::info!("<VirtIoNetDomain as DeviceBase>::handle_irq() called");
        self.nic
            .get_must()
            .lock()
            .ack_interrupt()
            .map_err(to_alien_err)?;
        Ok(())
    }
}

pub const NET_BUFFER_LEN: usize = 1600;

impl NetDeviceDomain for VirtIoNetDomain {
    fn init(&self, init_info: &VirtioInitInfo) -> AlienResult<()> {
        let transport = transport_from_init_info(init_info, DeviceType::Network, "virtio_net");
        let net = VirtIONet::new(transport, NET_BUFFER_LEN).expect("failed to create virtio net");
        self.nic.call_once(|| Mutex::new(net));
        Ok(())
    }

    fn mac_address(&self) -> AlienResult<[u8; 6]> {
        self.nic
            .get_must()
            .lock()
            .mac_address()
            .map_err(to_alien_err)
    }

    fn can_transmit(&self) -> AlienResult<bool> {
        self.nic.get_must().lock().can_send().map_err(to_alien_err)
    }

    fn can_receive(&self) -> AlienResult<bool> {
        Ok(self
            .nic
            .get_must()
            .lock()
            .can_recv()
            .map_err(to_alien_err)?
            .is_some())
    }

    fn rx_queue_size(&self) -> AlienResult<usize> {
        Ok(NET_QUEUE_SIZE)
    }

    fn tx_queue_size(&self) -> AlienResult<usize> {
        Ok(NET_QUEUE_SIZE)
    }

    fn transmit(&self, tx_buf: &DVec<u8>) -> AlienResult<()> {
        self.nic
            .get_must()
            .lock()
            .send(tx_buf.as_slice())
            .map_err(to_alien_err)
    }

    fn receive(&self, mut rx_buf: DVec<u8>) -> AlienResult<(DVec<u8>, usize)> {
        let len = self
            .nic
            .get_must()
            .lock()
            .receive(rx_buf.as_mut_slice())
            .map_err(to_alien_err)?;
        Ok((rx_buf, len))
    }
}

define_unwind_for_NetDeviceDomain!(VirtIoNetDomain);

pub fn main() -> Box<dyn NetDeviceDomain> {
    Box::new(UnwindWrap::new(VirtIoNetDomain::default()))
}
