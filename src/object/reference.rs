use anyhow::Context;
use bytes::Bytes;
use std::path::PathBuf;

/// Refs are text files containing a hexadecimal representation of an object’s hash, encoded in ASCII.
///
/// Refs can also refer to another reference, and thus only indirectly to an object.
pub struct Ref {
    _path: String,
    _ty: RefType,
}

#[derive(PartialEq)]
pub enum RefType {
    Symbolic,
    Direct,
}

impl Ref {
    pub fn resolve(path: &str) -> anyhow::Result<PathBuf> {
        let path = path.trim_end_matches('\n');
        if path.starts_with("ref: ") {
            let path = path.trim_start_matches("ref: ");
            let path = std::fs::read_to_string(path).context(format!("failed to read {}", path))?;
            Ref::resolve(&path)
        } else {
            Ok(PathBuf::from(path))
        }
    }

    pub fn parse(bytes: Bytes) -> anyhow::Result<Ref> {
        let ty = if bytes.starts_with(b"ref: ") {
            RefType::Symbolic
        } else {
            RefType::Direct
        };

        Ok(Ref {
            _path: String::from_utf8_lossy(&bytes).to_string(),
            _ty: ty,
        })
    }
}
