use crate::objects::kvlm::Kvlm;
use crate::objects::GitObjectTrait;
use bytes::Bytes;
use chrono::{DateTime, Offset};

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

    pub fn new(
        tree: String,
        parent: Option<String>,
        author: String,
        time: DateTime<chrono::Local>,
        message: String,
    ) -> Self {
        let mut kvlm = Kvlm::default();

        kvlm.insert("tree".to_string(), vec![tree]);

        parent.map(|parent| {
            kvlm.insert("parent".to_string(), vec![parent]);
            Some(())
        });

        let offset = time.offset().fix().local_minus_utc();

        let hours = offset / 3600;
        let minutes = (offset % 3600) / 60;

        let tz = format!("{:>+03}{:02}", hours, minutes);

        let time = format!("{} {}", time.timestamp(), tz);

        kvlm.insert("author".to_string(), vec![format!("{} {}", author, time)]);
        kvlm.insert(
            "committer".to_string(),
            vec![format!("{} {}", author, time)],
        );
        kvlm.insert("message".to_string(), vec![message]);

        Self { kvlm }
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
