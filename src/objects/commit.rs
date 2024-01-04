use crate::objects::kvlm::Kvlm;
use bytes::Bytes;
use crate::objects::GitObjectTrait;

/// A tree object, the contents of a worktree, files and directories;
/// contains following fields:
///
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
    kvlm: Kvlm,
}

impl Commit {
    impl_kvlm_getter_single! {
        tree,
        message,
        author,
        committer
    }

    pub fn parents(&self) -> Option<&Vec<String>> {
        self.kvlm.get("parent")
    }
}

impl GitObjectTrait for Commit {
    fn from_bytes(bytes: Bytes) -> anyhow::Result<Self> {
        Ok(Commit {
            kvlm: Kvlm::parse(bytes)?,
        })
    }

    fn serialize(&self) -> anyhow::Result<Bytes> {
        Ok(self.kvlm.serialize())
    }
}
