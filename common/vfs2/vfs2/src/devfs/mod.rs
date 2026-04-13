mod id;

use alloc::sync::Arc;
use alloc::{string::String, string::ToString};

use basic::println;
use id::alloc_device_id;
use interface::{DevFsDomain, DomainType};
use shared_heap::DVec;
use vfscore::{dentry::VfsDentry, utils::VfsNodeType};

///```bash
/// |
/// |-- null
/// |-- zero
/// |-- random
/// |-- urandom
/// |-- tty
/// |-- shm (a ramfs will be mounted here)
/// |-- misc
///    |-- rtc
/// ```
pub fn init_devfs(devfs_domain: &Arc<dyn DevFsDomain>, root_dt: &Arc<dyn VfsDentry>) {
    let root_inode = root_dt.inode().unwrap();

    let null_device_id = alloc_device_id(VfsNodeType::CharDevice);
    let random_device_id = alloc_device_id(VfsNodeType::CharDevice);
    devfs_domain
        .register(null_device_id.id(), &DVec::from_slice(b"null"))
        .unwrap();
    devfs_domain
        .register(random_device_id.id(), &DVec::from_slice(b"random"))
        .unwrap();
    root_inode
        .create(
            "null",
            'c'.into(),
            "rw-rw-rw-".into(),
            Some(null_device_id.id()),
        )
        .unwrap();
    root_inode
        .create(
            "zero",
            'c'.into(),
            "rw-rw-rw-".into(),
            Some(null_device_id.id()),
        )
        .unwrap();
    root_inode
        .create(
            "random",
            'c'.into(),
            "rw-rw-rw-".into(),
            Some(random_device_id.id()),
        )
        .unwrap();
    root_inode
        .create(
            "urandom",
            'c'.into(),
            "rw-rw-rw-".into(),
            Some(random_device_id.id()),
        )
        .unwrap();

    root_inode
        .create("shm", VfsNodeType::Dir, "rwxrwxrwx".into(), None)
        .unwrap();
    root_inode
        .create("misc", VfsNodeType::Dir, "rwxrwxrwx".into(), None)
        .unwrap();

    scan_system_devices(devfs_domain, root_dt);

    // todo!(tty,shm,misc)
    println!("devfs init success");
}

pub fn scan_system_devices(devfs_domain: &Arc<dyn DevFsDomain>, root_dt: &Arc<dyn VfsDentry>) {
    let root = root_dt.inode().unwrap();

    let uart = basic::get_domain("buf_uart"); // unique name
    let gpu = find_domain_name(&[
        "gpu",
        "gpu-1",
        "virtio_gpu-1",
        "virtio_gpu",
    ]);
    let mouse = find_domain_name(&["mouse", "buf_input-2", "buf_input-1"]);
    let keyboard = find_domain_name(&["keyboard", "buf_input-1", "buf_input-2"]);
    let blk = basic::get_domain("cache_blk-1");
    let rtc = find_rtc_domain_name(&["rtc", "goldfish"]);

    match uart {
        Some(_) => {
            let uart_id = alloc_device_id(VfsNodeType::CharDevice);
            devfs_domain
                .register(uart_id.id(), &DVec::from_slice(b"buf_uart"))
                .unwrap();
            root.create(
                "tty",
                VfsNodeType::CharDevice,
                "rw-rw----".into(),
                Some(uart_id.id()),
            )
            .unwrap();
        }
        None => {
            panic!("uart domain not found");
        }
    }

    match gpu {
        Some(gpu_name) => {
            let gpu_id = alloc_device_id(VfsNodeType::CharDevice);
            devfs_domain
                .register(gpu_id.id(), &DVec::from_slice(gpu_name.as_bytes()))
                .unwrap();
            root.create(
                "gpu",
                VfsNodeType::CharDevice,
                "rw-rw----".into(),
                Some(gpu_id.id()),
            )
            .unwrap();
        }
        None => {
            println!("gpu domain not found");
        }
    }

    match mouse {
        Some(mouse_name) => {
            let mouse_id = alloc_device_id(VfsNodeType::CharDevice);
            devfs_domain
                .register(mouse_id.id(), &DVec::from_slice(mouse_name.as_bytes()))
                .unwrap();
            root.create(
                "mouse",
                VfsNodeType::CharDevice,
                "rw-rw----".into(),
                Some(mouse_id.id()),
            )
            .unwrap();
        }
        None => {
            println!("mouse domain not found");
        }
    }

    match keyboard {
        Some(keyboard_name) => {
            let keyboard_id = alloc_device_id(VfsNodeType::CharDevice);
            devfs_domain
                .register(keyboard_id.id(), &DVec::from_slice(keyboard_name.as_bytes()))
                .unwrap();
            root.create(
                "keyboard",
                VfsNodeType::CharDevice,
                "rw-rw----".into(),
                Some(keyboard_id.id()),
            )
            .unwrap();
        }
        None => {
            println!("keyboard domain not found");
        }
    }

    match blk {
        Some(_) => {
            let blk_id = alloc_device_id(VfsNodeType::BlockDevice);
            devfs_domain
                .register(blk_id.id(), &DVec::from_slice(b"cache_blk-1"))
                .unwrap();
            root.create(
                "sda",
                VfsNodeType::BlockDevice,
                "rw-rw----".into(),
                Some(blk_id.id()),
            )
            .unwrap();
        }
        None => panic!("blk domain not found"),
    }

    match rtc {
        Some(rtc_name) => {
            let rtc_id = alloc_device_id(VfsNodeType::CharDevice);
            devfs_domain
                .register(rtc_id.id(), &DVec::from_slice(rtc_name.as_bytes()))
                .unwrap();
            root.create(
                "rtc",
                VfsNodeType::CharDevice,
                "rw-rw----".into(),
                Some(rtc_id.id()),
            )
            .unwrap();
        }
        None => {
            println!("rtc domain not found");
        }
    };
}

fn find_domain_name(candidates: &[&str]) -> Option<String> {
    for name in candidates {
        match basic::get_domain(name) {
            Some(DomainType::GpuDomain(_)) | Some(DomainType::BufInputDomain(_)) => {
                return Some((*name).to_string());
            }
            _ => {}
        }
    }
    None
}

fn find_rtc_domain_name(candidates: &[&str]) -> Option<String> {
    for name in candidates {
        if matches!(basic::get_domain(name), Some(DomainType::RtcDomain(_))) {
            return Some((*name).to_string());
        }
    }
    None
}
