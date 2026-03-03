use std::{env, fs, path::Path, process};

use crate::subcommand::{Config, DOMAIN_SET};

/// Valid architectures
const VALID_ARCHS: [&str; 3] = ["riscv64", "x86_64", "plat_vf2"];

/// Get target configuration based on ARCH environment variable
fn get_target_config() -> (&'static str, &'static str) {
    let arch = env::var("ARCH").unwrap_or_else(|_| "riscv64".to_string());
    
    // Validate architecture
    if !VALID_ARCHS.contains(&arch.as_str()) {
        eprintln!("Error: Invalid ARCH='{}'. Valid values are: {:?}", arch, VALID_ARCHS);
        process::exit(1);
    }
    
    match arch.as_str() {
        "x86_64" => ("./x86_64.json", "x86_64"),
        "riscv64" | "plat_vf2" => ("./riscv64.json", "riscv64"),
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

pub fn build_single(name: &str, log: &str, output: &String) {
    check_output_exist(output);
    let domain_list = fs::read_to_string("./domain-list.toml").unwrap();
    let config: Config = toml::from_str(&domain_list).unwrap();
    let all_members = config.domains.get("members").unwrap();
    let r_name = name;
    if !all_members.contains(&r_name.to_string()) {
        println!(
            "Domain [{}] is not in the members list, skip building",
            r_name
        );
        return;
    }
    let init_members = config.domains.get("init_members").unwrap();
    if init_members.contains(&r_name.to_string()) {
        build_domain(r_name, log.to_string(), "init", output);
    } else {
        let disk_members = config.domains.get("disk_members").unwrap();
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
    println!("Building domain [{}] project for target: {}", name, target_dir);
    for ty in DOMAIN_SET {
        let path = format!("./{}/{}/g{}/Cargo.toml", ty, name, name);
        let path = Path::new(&path);
        if path.exists() {
            let path = format!("./{}/{}/g{}/Cargo.toml", ty, name, name);
            let path = Path::new(&path);
            println!("Start building domain,path: {:?}", path);
            let _cmd = std::process::Command::new("cargo")
                .arg("build")
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
    let all_members = config.domains.get("members").unwrap().clone();
    let init_members = config.domains.get("init_members").unwrap().clone();
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
    let disk_members = config.domains.get("disk_members").unwrap().clone();
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
