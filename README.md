# Domains

Domains are isolated components in AlienOS. Each domain is a separate Rust project that can be loaded/unloaded/update at runtime. 
Domains can be categorized into three types: Common, Fs, and Driver. 

- Common domains are used to provide common functionalities, such as syscall, memory, and process management. 
- Fs domains are used to provide file system functionalities, such as devfs, dynfs, ramfs, fat-vfs, and domainfs. 
- Driver domains are used to provide device driver functionalities, such as uart8250, virtio-net, visionfive2-sd, plic, and rtc.

## 架构差异说明

- riscv64 专属域：`plic`、`goldfish`、`vf2_sd`（`vf2_sd` 仅 VF2 平台使用）。
- x86_64 不应引入上述专属域，相关构建由 `domains/domain-list.toml` 的 `*_x86_64` 列表控制。
- 通用域优先保持同名同职责，按需通过 `#[cfg(target_arch = ...)]` 在域内做细分实现。


See [AlienOS](https://github.com/Godones/Alien/tree/isolation) to know how to load/unload/update a domain.

## Introduction


## Create a new Domain

cd to domains directory

1. run cargo command

   ```
   cargo domain new --name {domain_name}
   ```

2. choose the domain type

   ```
   1. Common
   2. Fs
   3. Driver
   ```
3. input the domain interface name

   ```
   {interface_name}
   ```

4. update domain-list.toml


## Build
```
cargo domain --help # Display help
cargo domain build-all -l "" # Build all domains
cargo domain build -n syscall -l "" # Build syscall domain
```
