pub mod blob;
pub mod commit;
pub mod kvlm;
pub mod tag;
pub mod tree;

use crate::repository::Repository;
use crate::utils::sha;
use anyhow::Context;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use clap::ValueEnum;
use std::io::{Read, Write};

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
    pub fn new(fmt: Fmt, data: Vec<u8>) -> GitObject {
        let length = data.len();

        let header = Header { fmt, length };

        GitObject {
            header,
            data: data.into(),
        }
    }

    pub fn read_object(repo: &Repository, sha: &str) -> anyhow::Result<GitObject> {
        let path = repo.git_dir.join("objects").join(&sha[..2]).join(&sha[2..]);

        anyhow::ensure!(path.exists(), "objects not found: {}", sha);

        let file = std::fs::File::open(&path)?;

        let mut data = Vec::new();
        flate2::bufread::ZlibDecoder::new_with_decompress(
            std::io::BufReader::new(file),
            flate2::Decompress::new(true),
        )
        .read_to_end(&mut data)
        .context("failed to read zlib data")?;

        let mut data = Bytes::from(data);

        let (fmt, rest) = data
            .split_once(|&x| x == b' ')
            .context("failed to split objects fmt")?;
        let (length, rest) = rest
            .split_once(|&x| x == b'\0')
            .context("failed to split objects length")?;

        let fmt = std::str::from_utf8(fmt).context("failed to parse objects fmt")?;

        let fmt = Fmt::from_str(fmt, true)
            .map_err(|e| anyhow::anyhow!(e))
            .context(format!(
                "failed to parse objects fmt {} for sha {}",
                fmt, sha
            ))?;

        let length = std::str::from_utf8(length).context("failed to parse objects length")?;

        let length = length
            .parse::<usize>()
            .context("failed to parse objects length")?;

        anyhow::ensure!(rest.len() == length, "objects length mismatch");

        data.advance(data.len() - rest.len());

        let header = Header { fmt, length };

        Ok(GitObject { header, data })
    }

    /// write objects to disk
    ///
    /// returns sha of objects
    pub fn write_object(&self, repo: &Repository) -> anyhow::Result<String> {
        let data = self.serialize();

        let sha = sha(&data);

        let path = repo.git_dir.join("objects").join(&sha[..2]).join(&sha[2..]);

        anyhow::ensure!(!path.exists(), "objects already exists: {}", sha);

        std::fs::create_dir_all(
            path.parent()
                .context(format!("failed to get path parent: {}", path.display()))?,
        )?;

        let file = std::fs::File::create(&path)?;

        let mut encoder = flate2::write::ZlibEncoder::new(file, flate2::Compression::default());

        encoder
            .write_all(&data)
            .context("failed to write zlib data")?;

        encoder.finish().context("failed to write zlib data")?;

        Ok(sha)
    }

    pub fn serialize(&self) -> Bytes {
        // todo still clones data here :(
        // maybe change the GitObject
        let mut data = BytesMut::new();

        data.extend_from_slice(self.header.fmt.to_str().as_bytes());
        data.put_u8(b' ');
        data.extend_from_slice(self.header.length.to_string().as_bytes());
        data.put_u8(b'\0');
        data.extend_from_slice(&self.data);

        data.into()
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
