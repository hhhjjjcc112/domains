use std::{env, fs, path::{Path, PathBuf}, process};

use crate::subcommand::{pick_domain_list, Config, DOMAIN_SET};

/// Valid architectures
const VALID_ARCHS: [&str; 3] = ["riscv64", "x86_64", "vf2"];
const VALID_PLATFORMS: [&str; 3] = ["plat_qemu_riscv", "plat_qemu_x86_64", "plat_vf2"];

/// Get target configuration based on ARCH environment variable
fn get_target_config() -> (&'static str, &'static str) {
    let arch = env::var("ARCH").unwrap_or_else(|_| "riscv64".to_string());
    let platform = env::var("PLATFORM").ok();
    
    // Validate architecture
    if !VALID_ARCHS.contains(&arch.as_str()) {
        eprintln!("Error: Invalid ARCH='{}'. Valid values are: {:?}", arch, VALID_ARCHS);
        process::exit(1);
    }

    if let Some(ref plat) = platform {
        if !VALID_PLATFORMS.contains(&plat.as_str()) {
            eprintln!(
                "Error: Invalid PLATFORM='{}'. Valid values are: {:?}",
                plat, VALID_PLATFORMS
            );
            process::exit(1);
        }
    }

    // Backward-compatible ARCH=vf2 alias.
    let arch_kind = if arch == "x86_64" { "x86_64" } else { "riscv64" };
    let default_platform = if arch == "vf2" {
        "plat_vf2"
    } else if arch == "x86_64" {
        "plat_qemu_x86_64"
    } else {
        "plat_qemu_riscv"
    };
    let platform = platform.unwrap_or_else(|| default_platform.to_string());

    let valid_combo = match arch_kind {
        "x86_64" => platform == "plat_qemu_x86_64",
        "riscv64" => matches!(platform.as_str(), "plat_qemu_riscv" | "plat_vf2"),
        _ => false,
    };
    if !valid_combo {
        eprintln!(
            "Error: Invalid ARCH/PLATFORM combination: ARCH='{}', PLATFORM='{}'",
            arch, platform
        );
        process::exit(1);
    }
    
    match arch_kind {
        "x86_64" => ("./x86_64.json", "x86_64"),
        "riscv64" => ("./riscv64.json", "riscv64"),
        _ => unreachable!(),
    }
}

fn check_output_exist(output: &String) {
    let disk_path = format!("{}/disk", output);
    let init_path = format!("{}/init", output);
    let disk_dir = Path::new(disk_path.as_str());
    let init_dir = Path::new(init_path.as_str());
    if !disk_dir.exists() || !init_dir.exists() {
        println!("Output directory not exist, creating...");
        fs::create_dir_all(&format!("{}/disk", output)).unwrap();
        fs::create_dir_all(&format!("{}/init", output)).unwrap();
    }
}

fn gen_domain_linker_script(target_dir: &str) -> PathBuf {
    let output_arch = match target_dir {
        "x86_64" => "i386:x86-64",
        "riscv64" => "riscv",
        _ => unreachable!(),
    };
    let template_path = Path::new("./domain.ld");
    let template = fs::read_to_string(template_path).expect("failed to read domain.ld template");
    let linker_script = template
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("OUTPUT_ARCH(") {
                format!("OUTPUT_ARCH({})", output_arch)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let output_path = PathBuf::from(format!("./target/domain-{}.ld", target_dir));
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).expect("failed to create target directory for ld script");
    }
    fs::write(&output_path, linker_script).expect("failed to generate temporary domain linker script");
    output_path
}

pub fn build_single(name: &str, log: &str, output: &String) {
    check_output_exist(output);
    let domain_list = fs::read_to_string("./domain-list.toml").unwrap();
    let config: Config = toml::from_str(&domain_list).unwrap();
    let all_members = pick_domain_list(&config, "members");
    let r_name = name;
    if !all_members.contains(&r_name.to_string()) {
        println!(
            "Domain [{}] is not in the members list, skip building",
            r_name
        );
        return;
    }
    let init_members = pick_domain_list(&config, "init_members");
    if init_members.contains(&r_name.to_string()) {
        build_domain(r_name, log.to_string(), "init", output);
    } else {
        let disk_members = pick_domain_list(&config, "disk_members");
        if disk_members.contains(&r_name.to_string()) {
            build_domain(r_name, log.to_string(), "disk", output);
        } else {
            println!(
                "Domain [{}] is not in the init or disk members list, skip building",
                r_name
            );
        }
    }
}

pub fn build_domain(name: &str, log: String, dir: &str, output: &String) {
    let (target_json, target_dir) = get_target_config();
    let linker_script = gen_domain_linker_script(target_dir);
    let linker_script = fs::canonicalize(linker_script)
        .expect("failed to canonicalize temporary domain linker script");
    println!("Building domain [{}] project for target: {}", name, target_dir);
    for ty in DOMAIN_SET {
        let path = format!("./{}/{}/g{}/Cargo.toml", ty, name, name);
        let path = Path::new(&path);
        if path.exists() {
            let path = format!("./{}/{}/g{}/Cargo.toml", ty, name, name);
            let path = Path::new(&path);
            println!("Start building domain,path: {:?}", path);
            // 仅动态传入临时链接脚本，其他编译参数保持在 .cargo/config.toml。
            let _cmd = std::process::Command::new("cargo")
                .arg("rustc")
                .arg("--release")
                .env("LOG", log)
                .arg("--manifest-path")
                .arg(path)
                .arg("--target")
                .arg(target_json)
                .arg("-Zbuild-std=core,alloc")
                .arg("-Zbuild-std-features=compiler-builtins-mem")
                .arg("--target-dir")
                .arg("./target")
                .arg("--")
                .arg(format!("-Clink-arg=-T{}", linker_script.display()))
                .status()
                .expect("failed to execute cargo build");
            println!("Build domain [{}] project success", name);
            std::process::Command::new("cp")
                .arg(format!("./target/{}/release/g{}", target_dir, name))
                .arg(format!("{}/{}/g{}", output, dir, name))
                .status()
                .expect("failed to execute cp");
            println!("Copy domain [{}] project success", name);
            return;
        }
    }
}

pub fn build_all(log: String, output: &String) {
    check_output_exist(output);
    let domain_list = fs::read_to_string("./domain-list.toml").unwrap();
    let config: Config = toml::from_str(&domain_list).unwrap();
    println!("Start building all domains");
    let all_members = pick_domain_list(&config, "members");
    let init_members = pick_domain_list(&config, "init_members");
    for domain_name in init_members {
        if !all_members.contains(&domain_name) {
            println!(
                "Domain [{}] is not in the members list, skip building",
                domain_name
            );
            continue;
        }
        let value = log.to_string();
        build_domain(&domain_name, value, "init", output)
    }
    let disk_members = pick_domain_list(&config, "disk_members");
    if !disk_members.is_empty() {
        for domain_name in disk_members {
            if !all_members.contains(&domain_name) {
                println!(
                    "Domain [{}] is not in the members list, skip building",
                    domain_name
                );
                continue;
            }
            let value = log.to_string();

            build_domain(&domain_name, value, "disk", output)
        }
    }
}
