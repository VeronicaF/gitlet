use crate::objects::kvlm::Kvlm;
use bytes::Bytes;
use crate::objects::GitObjectTrait;

/// A Tag object contains following fields:
///
/// 1. tag: the name of the tag;
/// 2. object: the object the tag points to;
/// 3. tagger: the identity of the person who created the tag;
/// 4. message: the message associated with the tag.
///
/// object can be a commit, a tree, a blob.
pub struct Tag {
    kvlm: Kvlm,
}

impl Tag {
    impl_kvlm_getter_single! {
        tag,
        object,
        tagger,
        message
    }

    pub fn new(tag: String, object: String, tagger: String, message: String) -> Self {
        let mut kvlm = Kvlm::default();
        kvlm.insert("objects".to_string(), vec![object]);
        kvlm.insert("type".to_string(), vec!["commit".to_string()]);
        kvlm.insert("tag".to_string(), vec![tag]);
        kvlm.insert("tagger".to_string(), vec![tagger]);
        kvlm.insert("message".to_string(), vec![message]);

        Self { kvlm }
    }
}

impl GitObjectTrait for Tag {
    fn from_bytes(data: Bytes) -> anyhow::Result<Self> {
        let kvlm = Kvlm::parse(data)?;

        anyhow::ensure!(kvlm.contains_key("objects"), "missing field objects");
        anyhow::ensure!(kvlm.contains_key("type"), "missing field type");
        anyhow::ensure!(kvlm.contains_key("tag"), "missing field tag");
        anyhow::ensure!(kvlm.contains_key("tagger"), "missing field tagger");
        anyhow::ensure!(kvlm.contains_key("message"), "missing field message");

        Ok(Self { kvlm })
    }

    fn serialize(&self) -> anyhow::Result<Bytes> {
        Ok(self.kvlm.serialize())
    }
}
