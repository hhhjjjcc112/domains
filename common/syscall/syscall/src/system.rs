use alloc::{sync::Arc, vec};
use core::sync::atomic::{AtomicU64, Ordering};

use basic::{AlienError, AlienResult};
use interface::TaskDomain;
use oorandom::Rand64;
use pconst::{GRND_NONBLOCK, GRND_RANDOM};
use pod::Pod;

static RANDOM_STATE: AtomicU64 = AtomicU64::new(0x4d59_5df4_d0f3_3173);

#[cfg(target_arch = "x86_64")]
const MACHINE: &str = "x86_64";
#[cfg(target_arch = "riscv64")]
const MACHINE: &str = "riscv64";

/// uname：把系统标识写入 `utsname`，`utsname` 是用户态输出缓冲区。
pub fn sys_uname(task_domain: &Arc<dyn TaskDomain>, utsname: usize) -> AlienResult<isize> {
    let info = system_info();
    task_domain.copy_to_user(utsname, info.as_bytes())?;
    Ok(0)
}
#[repr(C)]
#[derive(Copy, Clone, Pod)]
pub struct Utsname {
    /// 操作系统名
    sysname: [u8; 65],
    /// Name within communications network to which the node is attached
    nodename: [u8; 65],
    /// 系统发行版
    release: [u8; 65],
    /// 系统版本
    version: [u8; 65],
    /// 硬件类型
    machine: [u8; 65],
    /// 域名
    domainname: [u8; 65],
}
fn system_info() -> Utsname {
    const SYSNAME: &str = "Linux";
    const NODENAME: &str = "Alien";
    const RELEASE: &str = "5.1";
    const VERSION: &str = "5.1";
    const DOMAINNAME: &str = "RustOS";
    let mut name = Utsname {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
        domainname: [0; 65],
    };
    name.sysname[..SYSNAME.len()].copy_from_slice(SYSNAME.as_bytes());
    name.nodename[..NODENAME.len()].copy_from_slice(NODENAME.as_bytes());
    name.release[..RELEASE.len()].copy_from_slice(RELEASE.as_bytes());
    name.version[..VERSION.len()].copy_from_slice(VERSION.as_bytes());
    name.machine[..MACHINE.len()].copy_from_slice(MACHINE.as_bytes());
    name.domainname[..DOMAINNAME.len()].copy_from_slice(DOMAINNAME.as_bytes());
    name
}

/// getrandom：`buf` 是输出缓冲区，`len` 是长度，`flags` 仅接受 `GRND_NONBLOCK` 和 `GRND_RANDOM`。
pub fn sys_random(
    task_domain: &Arc<dyn TaskDomain>,
    buf: usize,
    len: usize,
    flags: usize,
) -> AlienResult<isize> {
    validate_random_flags(flags)?;
    if len == 0 {
        return Ok(0);
    }

    let mut random_buf = vec![0; random_length(len, flags)];
    fill_random_bytes(&mut random_buf);
    task_domain.copy_to_user(buf, &random_buf)?;
    Ok(random_buf.len() as isize)
}

fn fill_random_bytes(buf: &mut [u8]) {
    let mut rng = Rand64::new(seed_material());
    for chunk in buf.chunks_mut(8) {
        let bytes = rng.rand_u64().to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
}

fn validate_random_flags(flags: usize) -> AlienResult<()> {
    let allowed_flags = GRND_NONBLOCK | GRND_RANDOM;
    if flags & !allowed_flags != 0 {
        return Err(AlienError::EINVAL);
    }
    Ok(())
}

fn random_length(len: usize, flags: usize) -> usize {
    if flags & GRND_RANDOM != 0 {
        core::cmp::min(len, 512)
    } else {
        len
    }
}

fn seed_material() -> u128 {
    let seed = next_seed();
    let seed_hi = splitmix64(seed ^ 0x9e37_79b9_7f4a_7c15);
    ((seed as u128) << 64) | u128::from(seed_hi)
}

fn next_seed() -> u64 {
    let time_ns = basic::time::read_time_ns();
    let ticks = basic::time::current_ticks();
    let timer = basic::time::read_timer() as u64;
    let counter = RANDOM_STATE.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);
    #[cfg(target_arch = "x86_64")]
    {
        let mut seed =
            counter ^ time_ns.rotate_left(13) ^ ticks.rotate_left(7) ^ timer.rotate_left(29);
        seed ^= x86_64_hardware_entropy();
        if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        let seed = counter ^ time_ns.rotate_left(13) ^ ticks.rotate_left(7) ^ timer.rotate_left(29);
        if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        }
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut mixed = value;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

#[cfg(target_arch = "x86_64")]
fn x86_64_hardware_entropy() -> u64 {
    let ticks = basic::time::current_ticks();
    let timer = basic::time::read_timer() as u64;
    let mixed = ticks ^ timer.rotate_left(17);
    mixed ^ mixed.rotate_left(31)
}
