pub mod blob;
pub mod commit;
pub mod kvlm;
pub mod tag;
pub mod tree;

use anyhow::Context;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use clap::ValueEnum;
use std::io::Read;
use std::path::PathBuf;

/// Read and write git objects, do the serialization and deserialization with compression
#[derive(Debug)]
pub struct GitObject {
    pub header: Header,
    pub data: Bytes,
}

#[derive(Debug)]
pub struct Header {
    pub fmt: Fmt,
    pub length: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Fmt {
    Commit,
    Tree,
    Blob,
    Tag,
}

impl Fmt {
    pub fn to_str(&self) -> &str {
        match self {
            Fmt::Commit => "commit",
            Fmt::Tree => "tree",
            Fmt::Blob => "blob",
            Fmt::Tag => "tag",
        }
    }
}

impl GitObject {
    pub fn new(fmt: Fmt, data: Bytes) -> GitObject {
        let length = data.len();

        let header = Header { fmt, length };

        GitObject { header, data }
    }

    pub fn from_bytes(mut bytes: Bytes) -> anyhow::Result<Self> {
        let (fmt, rest) = bytes
            .split_once(|&x| x == b' ')
            .context("failed to split objects fmt")?;
        let (length, rest) = rest
            .split_once(|&x| x == b'\0')
            .context("failed to split objects length")?;

        let fmt = std::str::from_utf8(fmt).context("failed to parse objects fmt")?;

        let fmt = Fmt::from_str(fmt, true)
            .map_err(|e| anyhow::anyhow!(e))
            .context(format!("failed to parse objects fmt {}", fmt))?;

        let length = std::str::from_utf8(length).context("failed to parse objects length")?;

        let length = length
            .parse::<usize>()
            .context("failed to parse objects length")?;

        anyhow::ensure!(rest.len() == length, "objects length mismatch");

        bytes.advance(bytes.len() - rest.len());

        let header = Header { fmt, length };

        Ok(GitObject {
            header,
            data: bytes,
        })
    }

    pub fn from_file(path: impl Into<PathBuf>, fmt: Fmt) -> anyhow::Result<Self> {
        let mut file = std::fs::File::open(path.into())?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        Ok(Self::new(fmt, data.into()))
    }

    pub fn serialize(&self) -> anyhow::Result<Bytes> {
        // todo still clones data here :(
        // maybe change the GitObject
        let mut data = BytesMut::new();

        data.extend_from_slice(self.header.fmt.to_str().as_bytes());
        data.put_u8(b' ');
        data.extend_from_slice(self.header.length.to_string().as_bytes());
        data.put_u8(b'\0');
        data.extend_from_slice(&self.data);

        Ok(data.into())
    }
}

impl std::fmt::Display for GitObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.data))
    }
}

pub trait GitObjectTrait {
    fn from_bytes(data: Bytes) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn serialize(&self) -> anyhow::Result<Bytes>;
}
