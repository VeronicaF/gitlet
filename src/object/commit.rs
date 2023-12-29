use crate::object::kvlm::Kvlm;
use bytes::Bytes;

/// A tree object, which we’ll discuss now, that is, the contents of a worktree, files and directories;
/// Zero, one or more parents;
///
/// An author identity (name and email), and a timestamp;
///
/// A committer identity (name and email), and a timestamp;
///
/// An optional PGP signature
///
/// A message;
pub struct Commit {
    data: Bytes,
    kvlm: Kvlm,
}

impl Commit {
    pub fn new(bytes: &Bytes) -> Commit {
        Commit {
            data: bytes.clone(),
            kvlm: Kvlm::parse(bytes),
        }
    }

    pub fn message(&self) -> Option<String> {
        let bytes = self
            .kvlm
            .dict
            .get("message".as_bytes())
            .map(|v| v[0].clone())?;

        Some(String::from_utf8_lossy(&bytes).to_string())
    }

    pub fn parents(&self) -> Option<Vec<Bytes>> {
        self.kvlm.dict.get("parent".as_bytes()).cloned()
    }
}

impl std::fmt::Display for Commit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = String::from_utf8_lossy(&self.data);
        write!(f, "{}", data)
    }
}
