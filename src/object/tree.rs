use anyhow::Context;
use bytes::{BufMut, Bytes, BytesMut};
use std::path::PathBuf;

/// a tree describes the content of the work tree
///
/// it associates blobs to paths.
///
/// It’s an array of three-element tuples made of a file mode, a path (relative to the worktree) and a SHA-1.
pub struct Tree(pub Vec<(FileMode, PathBuf, Sha1)>);

pub struct FileMode(pub String);

pub struct Sha1 {
    pub file_type: FileType,
    pub sha1: String,
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

impl Tree {
    /// `[mode] space [path] 0x00 [sha-1]`
    /// `[mode]` is up to six bytes and is an octal representation of a file mode, stored in ASCII.
    /// The first two digits encode the file type (file, directory, symlink or submodule), the last four the permissions.
    ///
    /// It’s followed by 0x20, an ASCII space;
    ///
    /// Followed by the null-terminated (0x00) path;
    ///
    /// Followed by the object’s SHA-1 in binary encoding, on 20 bytes.
    pub fn parse(bytes: Bytes) -> anyhow::Result<Self> {
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

                        let path =
                            PathBuf::from(String::from_utf8_lossy(&path.split()).to_string());
                        let sha1 = hex::encode(sha1.split());

                        let file_type = FileType::from_octal(&mode[0..2])?;

                        let sha1 = Sha1 { file_type, sha1 };

                        arr.push((FileMode(mode), path, sha1));
                    }
                }
            }
        }

        anyhow::ensure!(state == State::Init, "invalid tree");

        Ok(Tree(arr))
    }

    pub fn serialize(&mut self) -> anyhow::Result<Bytes> {
        let mut bytes = BytesMut::new();
        self.0.sort_by(|a, b| a.1.cmp(&b.1));

        for (mode, path, sha1) in &self.0 {
            bytes.put_slice(mode.0.as_bytes());
            bytes.put_u8(b' ');
            bytes.put_slice(path.to_str().context("invalid path")?.as_bytes());
            bytes.put_u8(b'\0');

            bytes.put_slice(hex::decode(&sha1.sha1).context("invalid sha1")?.as_slice());
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
        let mut tree = Tree::parse(raw.clone()).unwrap();

        assert_eq!(tree.0.len(), 1);
        assert_eq!(tree.0[0].0 .0, "100644");
        assert_eq!(tree.0[0].1.to_str().unwrap(), ".gitignore");
        assert_eq!(
            &tree.0[0].2.sha1,
            "be0c80f03e9bfa51999c6c8746b9e358124d53ef"
        );

        assert_eq!(tree.serialize().unwrap(), raw);
    }
}
