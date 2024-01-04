use bytes::Bytes;
use crate::objects::GitObjectTrait;

pub struct Blob {
    pub data: Bytes,
}

impl Blob {
    pub fn new(data: Bytes) -> Blob {
        Blob { data }
    }
}

impl GitObjectTrait for Blob {
    fn from_bytes(data: Bytes) -> anyhow::Result<Self> {
        Ok(Self { data })
    }

    fn serialize(&self) -> anyhow::Result<Bytes> {
        Ok(self.data.clone())
    }
}
