use crate::objects::GitObjectTrait;
use anyhow::Context;
use bytes::{BufMut, Bytes, BytesMut};
use std::path::PathBuf;

/// a tree describes the content of the work tree
///
/// it associates blobs to paths.
///
/// It’s an array of three-element tuples made of a file mode, a path (relative to the worktree) and a SHA-1.
#[derive(Default, Debug)]
pub struct Tree(pub Vec<TreeEntry>);

impl Tree {
    pub fn insert(&mut self, entry: TreeEntry) {
        self.0.push(entry);
    }
}

#[derive(Debug)]
pub struct TreeEntry {
    pub mode: String,
    pub path: PathBuf,
    pub sha1: String,
}

impl TreeEntry {
    pub fn try_new(mode: String, path: PathBuf, sha1: String) -> anyhow::Result<Self> {
        anyhow::ensure!(mode.len() <= 6, "invalid mode");

        let mode = format!("{:0>6}", mode);

        anyhow::ensure!(FileType::from_octal(&mode[0..2]).is_ok(), "invalid mode");

        Ok(Self { mode, path, sha1 })
    }

    pub fn file_type(&self) -> anyhow::Result<FileType> {
        FileType::from_octal(&self.mode[0..2])
    }
}
#[derive(PartialEq)]
pub enum FileType {
    Tree,
    Blob,
    SymLink,
    Commit,
}

impl FileType {
    pub fn from_octal(octal: &str) -> anyhow::Result<Self> {
        Ok(match octal {
            "04" => FileType::Tree,
            "10" => FileType::Blob,
            "12" => FileType::SymLink,
            "16" => FileType::Commit,
            _ => anyhow::bail!("unknown file type"),
        })
    }

    pub fn to_octal(&self) -> String {
        match self {
            FileType::Tree => "04",
            FileType::Blob => "10",
            FileType::SymLink => "12",
            FileType::Commit => "16",
        }
        .to_string()
    }

    pub fn to_str(&self) -> &str {
        match self {
            FileType::Tree => "tree",
            FileType::Blob => "blob",
            FileType::SymLink => "symLink",
            FileType::Commit => "commit",
        }
    }
}

impl Tree {}

impl GitObjectTrait for Tree {
    /// `[mode] space [path] 0x00 [sha-1]`
    /// `[mode]` is up to six bytes and is an octal representation of a file mode, stored in ASCII.
    /// The first two digits encode the file type (file, directory, symlink or submodule), the last four the permissions.
    ///
    /// It’s followed by 0x20, an ASCII space;
    ///
    /// Followed by the null-terminated (0x00) path;
    ///
    /// Followed by the objects’s SHA-1 in binary encoding, on 20 bytes.
    fn from_bytes(bytes: Bytes) -> anyhow::Result<Self> {
        #[derive(Debug, PartialEq)]
        enum State {
            Init,
            Mode,
            Path,
            Sha1,
        }

        let mut state = State::Init;

        let mut arr = Vec::new();

        let mut mode = BytesMut::new();
        let mut path = BytesMut::new();
        let mut sha1 = BytesMut::new();

        for byte in bytes {
            match state {
                State::Init => {
                    state = State::Mode;
                    mode.put_u8(byte);
                }
                State::Mode => {
                    if byte == b' ' {
                        state = State::Path;
                    } else {
                        mode.put_u8(byte);
                    }
                }
                State::Path => {
                    if byte == b'\0' {
                        state = State::Sha1;
                    } else {
                        path.put_u8(byte);
                    }
                }
                State::Sha1 => {
                    sha1.put_u8(byte);
                    if sha1.len() == 20 {
                        state = State::Init;
                        let mode =
                            format!("{:0>6}", String::from_utf8_lossy(&mode.split()).to_string());

                        anyhow::ensure!(FileType::from_octal(&mode[0..2]).is_ok(), "invalid mode");

                        let path =
                            PathBuf::from(String::from_utf8_lossy(&path.split()).to_string());
                        let sha1 = hex::encode(sha1.split());

                        arr.push(TreeEntry::try_new(mode, path, sha1)?);
                    }
                }
            }
        }

        anyhow::ensure!(state == State::Init, "invalid tree");

        Ok(Tree(arr))
    }

    fn serialize(&self) -> anyhow::Result<Bytes> {
        let mut bytes = BytesMut::new();

        let mut data: Vec<&TreeEntry> = self.0.iter().collect();

        data.sort_by(|a, b| {
            let path_a = a.path.clone();
            let path_b = b.path.clone();

            // unwrap is safe because we have checked the file type when creating the tree entry
            let mut path_a_str = path_a.to_str().unwrap().to_string();

            if let FileType::Tree = a.file_type().expect("invalid file type") {
                path_a_str.push('/');
            }

            let mut path_b_str = path_b.to_str().unwrap().to_string();

            if let FileType::Tree = b.file_type().expect("invalid file type") {
                path_b_str.push('/');
            }

            path_a_str.cmp(&path_b_str)
        });

        for TreeEntry { mode, path, sha1 } in data {
            bytes.put_slice(mode.trim_start_matches('0').as_bytes());
            bytes.put_u8(b' ');
            bytes.put_slice(path.to_str().context("invalid path")?.as_bytes());
            bytes.put_u8(b'\0');

            bytes.put_slice(hex::decode(sha1).context("invalid sha1")?.as_slice());
        }

        Ok(bytes.freeze())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tree_parse() {
        let mut raw = BytesMut::from("100644 .gitignore\0");
        raw.put_slice(
            hex::decode("be0c80f03e9bfa51999c6c8746b9e358124d53ef")
                .unwrap()
                .as_slice(),
        );
        let raw = raw.freeze();
        let tree = Tree::from_bytes(raw.clone()).unwrap();

        assert_eq!(tree.0.len(), 1);
        assert_eq!(tree.0[0].mode, "100644");
        assert_eq!(tree.0[0].path.to_str().unwrap(), ".gitignore");
        assert_eq!(&tree.0[0].sha1, "be0c80f03e9bfa51999c6c8746b9e358124d53ef");

        assert_eq!(tree.serialize().unwrap(), raw);
    }
}
