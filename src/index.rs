//! # The git index file
//! When the repository is “clean”, the index file holds the exact same contents as the HEAD commit, plus metadata about the corresponding filesystem entries. For instance, it may contain something like:
//!
//! > There’s a file called src/disp.c whose contents are blob 797441c76e59e28794458b39b0f1eff4c85f4fa0. The real src/disp.c file, in the worktree, was created on 2023-07-15 15:28:29.168572151, and last modified 2023-07-15 15:28:29.1689427709. It is stored on device 65026, inode 8922881.
//!
//! When you git add or git rm, the index file is modified accordingly. In the example above, if you modify src/disp.c, and add your changes, the index file will be updated with a new blob ID (the blob itself will also be created in the process, of course), and the various file metadata will be updated as well so git status knows when not to compare file contents.
//!
//! When you git commit those changes, a new tree is produced from the index file, a new commit object is generated with that tree, branches are updated and we’re done.

use bytes::{Buf, BufMut, Bytes, BytesMut};

/// # The git index file format
/// It is a **binary** file with three parts:
///
/// 1. An header with the `DIRC` magic bytes, a format version number and the number of entries the index holds;
/// 2. A series of entries, sorted, each representing a file; padded to multiple of 8 bytes.
/// 3. A series of optional extensions, which we’ll ignore.
#[derive(Debug)]
pub struct GitIndex {
    pub version: u32,
    pub entries: Vec<GitIndexEntry>,
}

impl GitIndex {
    pub fn from_bytes(mut bytes: Bytes) -> anyhow::Result<Self> {
        let mut header = bytes.split_to(12);

        let signature = header.split_to(4);
        anyhow::ensure!(&signature[..] == b"DIRC", "invalid index file signature");

        let version = header.split_to(4);
        let version = u32::from_be_bytes([version[0], version[1], version[2], version[3]]);
        // only support version 2 format
        anyhow::ensure!(version == 2, "invalid index file version");

        let num_entries = header.split_to(4);
        let num_entries = u32::from_be_bytes([
            num_entries[0],
            num_entries[1],
            num_entries[2],
            num_entries[3],
        ]);

        let mut entries = Vec::with_capacity(num_entries as usize);

        for _ in 0..num_entries {
            // Read creation time, as a unix timestamp (seconds since 1970-01-01 00:00:00, the "epoch")
            let ctime_sec = bytes.split_to(4);
            let ctime_sec =
                u32::from_be_bytes([ctime_sec[0], ctime_sec[1], ctime_sec[2], ctime_sec[3]]);

            // Read creation time, as a nanosecond offset from ctime_sec
            let ctime_nsec = bytes.split_to(4);
            let ctime_nsec =
                u32::from_be_bytes([ctime_nsec[0], ctime_nsec[1], ctime_nsec[2], ctime_nsec[3]]);

            // Read modification time, as a unix timestamp (seconds since 1970-01-01 00:00:00, the "epoch")
            let mtime_sec = bytes.split_to(4);
            let mtime_sec =
                u32::from_be_bytes([mtime_sec[0], mtime_sec[1], mtime_sec[2], mtime_sec[3]]);

            // Read modification time, as a nanosecond offset from mtime_sec
            let mtime_nsec = bytes.split_to(4);
            let mtime_nsec =
                u32::from_be_bytes([mtime_nsec[0], mtime_nsec[1], mtime_nsec[2], mtime_nsec[3]]);

            // Read device number of the device containing the file
            let dev = bytes.split_to(4);
            let dev = u32::from_be_bytes([dev[0], dev[1], dev[2], dev[3]]);

            // Read inode number of the file
            let ino = bytes.split_to(4);
            let ino = u32::from_be_bytes([ino[0], ino[1], ino[2], ino[3]]);

            // unused placeholder
            let unused = bytes.split_to(2);
            anyhow::ensure!(&unused[..] == [0, 0], "invalid index file format");

            // Read object type and permissions
            let mode = bytes.split_to(2);
            let mode = u16::from_be_bytes([mode[0], mode[1]]);
            let mode_type = mode >> 12;

            anyhow::ensure!(
                mode_type == 0b1000 || mode_type == 0b1010 || mode_type == 0b1110,
                "invalid index file format"
            );

            let mode_perms = mode & 0o0777;

            // Read user ID of owner
            let uid = bytes.split_to(4);
            let uid = u32::from_be_bytes([uid[0], uid[1], uid[2], uid[3]]);
            // Read group ID of owner
            let gid = bytes.split_to(4);
            let gid = u32::from_be_bytes([gid[0], gid[1], gid[2], gid[3]]);

            // Read size of file
            let fsize = bytes.split_to(4);
            let fsize = u32::from_be_bytes([fsize[0], fsize[1], fsize[2], fsize[3]]);

            // Read SHA-1 of object, we store it as a hex string in our struct.
            // In file it is stored as 20 bytes.

            let sha = bytes.split_to(20);
            let sha = hex::encode(sha);

            // Flags we're going to ignore
            let flags_and_name_len = bytes.split_to(2);
            let flags_and_name_len =
                u16::from_be_bytes([flags_and_name_len[0], flags_and_name_len[1]]);
            let flags = flags_and_name_len >> 12;

            let flag_assume_valid = (flags & 0b1000) != 0;
            let flag_extended = (flags & 0b0100) != 0;
            let flag_stage = flags & 0b0011;
            anyhow::ensure!(!flag_extended, "do not support extended flag");

            // Read name of file, null-terminated

            // Length of the name.  This is stored on 12 bits, some max
            // value is 0xFFF, 4095.  Since names can occasionally go
            // beyond that length, git treats 0xFFF as meaning at least
            //  0xFFF, and looks for the final 0x00 to find the end of the
            //  name --- at a small, and probably very rare, performance cost.
            let name_len = flags_and_name_len & 0x0fff;

            let name = if name_len < 0x0fff {
                anyhow::ensure!(
                    bytes.get(name_len as usize) == Some(&0),
                    "name is somehow not null-terminated"
                );

                let name = bytes.split_to(name_len as usize);
                bytes.advance(1); // null byte
                name
            } else {
                let mut name = BytesMut::with_capacity(0xfff + 1);
                loop {
                    let byte = bytes.get(0);
                    anyhow::ensure!(byte.is_some(), "name is somehow not null-terminated");
                    let byte = *byte.unwrap();
                    bytes.advance(1);
                    if byte == 0 {
                        break;
                    }
                    name.put_u8(byte);
                }
                name.freeze()
            };

            // We have consumed 62 + name.len() + 1 bytes
            let consumed = 62 + name.len() + 1;
            // We need to align to 8 bytes
            let padding = (8 - (consumed % 8)) % 8;
            bytes.advance(padding);

            let name = String::from_utf8_lossy(&name).to_string();

            let entry = GitIndexEntry {
                ctime: (ctime_sec, ctime_nsec),
                mtime: (mtime_sec, mtime_nsec),
                dev,
                ino,
                mode_type,
                mode_perms,
                uid,
                gid,
                fsize,
                sha,
                flag_assume_valid,
                flag_stage,
                name,
            };

            entries.push(entry);
        }

        Ok(GitIndex { version, entries })
    }
}

impl Default for GitIndex {
    fn default() -> Self {
        GitIndex {
            version: 2,
            entries: vec![],
        }
    }
}

/// # The git index file entry
#[derive(Debug)]
pub struct GitIndexEntry {
    /// The last time a file's metadata changed.  This is a pair
    /// (timestamp in seconds, nanoseconds)
    pub ctime: (u32, u32),
    /// The last time a file's data changed.  This is a pair
    /// (timestamp in seconds, nanoseconds)
    pub mtime: (u32, u32),
    /// The device number of the device containing the file.
    pub dev: u32,
    /// The inode number of the file.
    pub ino: u32,
    /// The object type, either b1000 (regular), b1010 (symlink), b1110 (gitlink).
    pub mode_type: u16,
    /// The object permission bits.
    pub mode_perms: u16,
    /// The user ID of the file's owner.
    pub uid: u32,
    /// The group ID of the file's owner.
    pub gid: u32,
    /// The size of the object, in bytes.
    pub fsize: u32,
    /// sha1 of the object
    pub sha: String,
    ///
    pub flag_assume_valid: bool,
    ///
    pub flag_stage: u16,
    ///
    pub name: String,
}

impl GitIndexEntry {
    pub fn mode_type_str(&self) -> &str {
        match self.mode_type {
            0b1000 => "regular file",
            0b1010 => "symlink",
            0b1110 => "git link",
            _ => unreachable!(),
        }
    }
}
