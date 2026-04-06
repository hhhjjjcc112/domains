use alloc::{sync::Arc, vec};

use basic::{println, AlienResult};
use core2::io::Read;
use cpio_reader::Mode;
use shared_heap::DVec;
use vfscore::{
    dentry::VfsDentry,
    path::VfsPath,
    utils::{VfsInodeMode, VfsNodeType},
};

pub fn populate_initrd(root: Arc<dyn VfsDentry>, initrd: &[u8]) -> AlienResult<()> {
    root.inode()?
        .create("bin", VfsNodeType::Dir, "rwxr-xr-x".into(), None)?;
    root.inode()?
        .create("sbin", VfsNodeType::Dir, "rwxr-xr-x".into(), None)?;
    root.inode()?
        .create("usr", VfsNodeType::Dir, "rwxr-xr-x".into(), None)?;
    let usr = root.find("usr").ok_or(basic::AlienError::ENOENT)?;
    usr.inode()?
        .create("bin", VfsNodeType::Dir, "rwxr-xr-x".into(), None)?;
    usr.inode()?
        .create("sbin", VfsNodeType::Dir, "rwxr-xr-x".into(), None)?;
    parse_initrd_data(root, initrd)?;
    println!("Initrd populate success");
    Ok(())
}
fn parse_initrd_data(root: Arc<dyn VfsDentry>, initrd: &[u8]) -> AlienResult<()> {
    let path = VfsPath::new(root.clone(), root.clone());
    let mut decoder = libflate::gzip::Decoder::new(initrd).unwrap();
    let mut buf = vec![];
    let _r = decoder.read_to_end(&mut buf).unwrap();
    for entry in cpio_reader::iter_files(&buf) {
        let mode = entry.mode();
        let name = entry.name();
        let is_cmd_path = name.starts_with("bin/")
            | name.starts_with("sbin/")
            | name.starts_with("usr/bin/")
            | name.starts_with("usr/sbin/");
        if is_cmd_path {
            let inode_mode = VfsInodeMode::from_bits_truncate(mode.bits());
            if mode.contains(Mode::SYMBOLIK_LINK) {
                // create symlink
                let data = entry.file();
                let target = core::str::from_utf8(data).unwrap();
                path.join(name)?.symlink(target)?;
            } else if mode.contains(Mode::REGULAR_FILE) {
                // 命令文件兜底补执行位，避免权限位丢失。
                let inode_mode = inode_mode
                    | VfsInodeMode::OWNER_EXEC
                    | VfsInodeMode::GROUP_EXEC
                    | VfsInodeMode::OTHER_EXEC;
                let f = path.join(name)?.open(Some(inode_mode))?;
                let data = DVec::from_other_rvec_slice(entry.file());
                f.inode()?.write_at(0, &data)?;
            }
        }
    }
    Ok(())
}
