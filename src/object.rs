pub mod blob;
pub mod commit;
mod kvlm;
pub mod tree;

use crate::repository::Repository;
use crate::utils::sha;
use anyhow::Context;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use clap::ValueEnum;
use std::io::{Read, Write};

/// read and write git objects, doing the serialization and deserialization with compression
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

        anyhow::ensure!(path.exists(), "object not found: {}", sha);

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
            .context("failed to split object fmt")?;
        let (length, rest) = rest
            .split_once(|&x| x == b'\0')
            .context("failed to split object length")?;

        let fmt = std::str::from_utf8(fmt).context("failed to parse object fmt")?;

        let fmt = Fmt::from_str(fmt, true)
            .map_err(|e| anyhow::anyhow!(e))
            .context(format!(
                "failed to parse object fmt {} for sha {}",
                fmt, sha
            ))?;

        let length = std::str::from_utf8(length).context("failed to parse object length")?;

        let length = length
            .parse::<usize>()
            .context("failed to parse object length")?;

        anyhow::ensure!(rest.len() == length, "object length mismatch");

        data.advance(data.len() - rest.len());

        let header = Header { fmt, length };

        Ok(GitObject { header, data })
    }

    /// write object to disk
    ///
    /// returns sha of object
    pub fn write_object(&self, repo: &Repository) -> anyhow::Result<String> {
        let data = self.serialize();

        let sha = sha(&data);

        let path = repo.git_dir.join("objects").join(&sha[..2]).join(&sha[2..]);

        anyhow::ensure!(!path.exists(), "object already exists: {}", sha);

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

    pub fn display(&self) -> String {
        match self.header.fmt {
            Fmt::Blob => {
                let blob = blob::Blob::new(&self.data);
                format!("{}", blob)
            }
            Fmt::Commit => {
                let commit = commit::Commit::new(&self.data);
                format!("{}", commit)
            }
            Fmt::Tree => {
                unimplemented!()
            }
            Fmt::Tag => {
                unimplemented!()
            }
        }
    }
}
