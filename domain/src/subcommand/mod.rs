use std::collections::BTreeMap;
use std::env;

use serde::Deserialize;

pub mod build;
pub mod clean;
pub mod fmt;
pub mod new;

#[derive(Deserialize)]
pub struct Config {
    pub domains: BTreeMap<String, Vec<String>>,
}

pub fn current_arch_kind() -> String {
    match env::var("ARCH").unwrap_or_else(|_| "riscv64".to_string()).as_str() {
        "x86_64" => "x86_64".to_string(),
        "vf2" | "riscv64" => "riscv64".to_string(),
        _ => "riscv64".to_string(),
    }
}

pub fn current_platform_kind() -> String {
    if let Ok(platform) = env::var("PLATFORM") {
        return platform;
    }
    match env::var("ARCH").unwrap_or_else(|_| "riscv64".to_string()).as_str() {
        "x86_64" => "plat_qemu_x86_64".to_string(),
        "vf2" => "plat_vf2".to_string(),
        _ => "plat_qemu_riscv".to_string(),
    }
}

pub fn pick_domain_list(config: &Config, base_key: &str) -> Vec<String> {
    let platform_key = format!("{}_{}", base_key, current_platform_kind());
    if let Some(list) = config.domains.get(&platform_key) {
        return list.clone();
    }
    let arch_key = format!("{}_{}", base_key, current_arch_kind());
    if let Some(list) = config.domains.get(&arch_key) {
        return list.clone();
    }
    config.domains.get(base_key).cloned().unwrap_or_default()
}

static DOMAIN_SET: [&str; 3] = ["common", "fs", "drivers"];
