use crate::object::kvlm::Kvlm;
use bytes::Bytes;

pub struct Commit {
    kvlm: Kvlm,
}

impl Commit {
    pub fn new(bytes: &Bytes) -> Commit {
        Commit {
            kvlm: Kvlm::parse(bytes),
        }
    }

    pub fn message(&self) -> Option<String> {
        self.kvlm.dict.get("message").map(|v| v[0].clone())
    }

    pub fn parents(&self) -> Option<Vec<String>> {
        self.kvlm.dict.get("parent").cloned()
    }
}
