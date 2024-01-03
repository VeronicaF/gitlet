use crate::object::kvlm::Kvlm;
use bytes::Bytes;

pub enum Tag {
    Lightweight { name: String, target: String },
    Annotated(AnnotatedTag),
}

pub struct AnnotatedTag {
    data: Bytes,
    kvlm: Kvlm,
}
