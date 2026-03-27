#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use core::{
    fmt::{Debug, Formatter, Result},
    ops::Range,
};

use basic::{
    io::SafeIORegion,
    println,
    sync::{Mutex, Once, OnceGet},
    AlienResult,
};
use interface::{define_unwind_for_NetDeviceDomain, Basic, DeviceBase, NetDeviceDomain};
use shared_heap::DVec;
#[cfg(target_arch = "riscv64")]
use virtio_drivers::transport::mmio::MmioTransport;
#[cfg(target_arch = "x86_64")]
use virtio_drivers::transport::{
    pci::{bus::{Cam, Command, PciRoot}, virtio_device_type, PciTransport},
    DeviceType,
};
use virtio_drivers::device::net::VirtIONet;
use virtio_mmio_common::{to_alien_err, HalImpl, SafeIORW};

pub const NET_QUEUE_SIZE: usize = 128;
pub const NET_BUF_LEN: usize = 4096;

#[derive(Default)]
pub struct VirtIoNetDomain {
    #[cfg(target_arch = "riscv64")]
    nic: Once<Mutex<VirtIONet<HalImpl, MmioTransport, NET_QUEUE_SIZE>>>,
    #[cfg(target_arch = "x86_64")]
    nic: Once<Mutex<VirtIONet<HalImpl, PciTransport, NET_QUEUE_SIZE>>>,
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
    fn init(&self, address_range: &Range<usize>) -> AlienResult<()> {
        #[cfg(target_arch = "riscv64")]
        {
            let io_region = SafeIORW(SafeIORegion::from(address_range.clone()));
            let transport = MmioTransport::new(Box::new(io_region)).unwrap();
            let net = VirtIONet::new(transport, NET_BUFFER_LEN)
                .expect("failed to create virtio net");
            self.nic.call_once(|| Mutex::new(net));
            return Ok(());
        }

        #[cfg(target_arch = "x86_64")]
        {
            let ecam = SafeIORW(SafeIORegion::from(address_range.clone()));
            let mut root = PciRoot::new(Box::new(ecam), Cam::Ecam);

            let mut found = None;
            'outer: for bus in 0u8..=u8::MAX {
                for (device_function, info) in root.enumerate_bus(bus) {
                    if virtio_device_type(&info) == Some(DeviceType::Network) {
                        found = Some((device_function, info));
                        break 'outer;
                    }
                }
            }

            let (device_function, info) = found.expect("virtio-pci net not found in ECAM");
            println!("virtio-pci net bdf={} ({})", device_function, info);

            let (_status, mut command) = root.get_status_command(device_function);
            command.insert(Command::BUS_MASTER | Command::MEMORY_SPACE | Command::IO_SPACE);
            root.set_command(device_function, command);

            let transport = PciTransport::new::<HalImpl>(&mut root, device_function)
                .expect("failed to create virtio-pci transport");
            let net = VirtIONet::new(transport, NET_BUFFER_LEN)
                .expect("failed to create virtio-pci net");
            self.nic.call_once(|| Mutex::new(net));
            return Ok(());
        }

        #[allow(unreachable_code)]
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
