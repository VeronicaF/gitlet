use bytes::{BufMut, Bytes, BytesMut};
use indexmap::IndexMap;
use std::ops::{Deref, DerefMut};

pub struct Kvlm {
    pub dict: IndexMap<Bytes, Vec<Bytes>>,
}

impl Default for Kvlm {
    fn default() -> Self {
        Self {
            dict: IndexMap::new(),
        }
    }
}

impl Kvlm {
    /// parse a key-value list with message
    pub fn parse(raw: &Bytes) -> Kvlm {
        enum KvlmState {
            Init,
            Key,
            Value,
            Message,
        }

        let mut state = KvlmState::Init;
        let mut key = BytesMut::new();
        let mut value = BytesMut::new();
        let mut message = BytesMut::new();
        let mut dict = IndexMap::<Bytes, Vec<Bytes>>::new();

        let mut index = 0usize;
        let len = raw.len();

        while index < len {
            let byte = raw[index];
            let next_byte = raw.get(index + 1);
            match state {
                KvlmState::Init => {
                    if byte == b'\n' {
                        state = KvlmState::Message;
                    } else {
                        state = KvlmState::Key;
                        key.put_u8(byte);
                    }
                }
                KvlmState::Key => {
                    if byte == b' ' {
                        state = KvlmState::Value;
                    } else {
                        key.put_u8(byte);
                    }
                }
                KvlmState::Value => {
                    if byte == b'\n' {
                        if next_byte == Some(&b' ') {
                            // Continuation lines
                            value.put_u8(b'\n');
                            index += 1;
                        } else {
                            let key = key.split().freeze();
                            let value = value.split().freeze();
                            dict.entry(key)
                                .and_modify(|v| v.push(value.clone()))
                                .or_insert(vec![value]);
                            state = KvlmState::Init;
                        }
                    } else {
                        value.put_u8(byte);
                    }
                }
                KvlmState::Message => {
                    message.put_u8(byte);
                }
            }
            index += 1;
        }

        let message = message.split().freeze();

        dict.entry(Bytes::from("message"))
            .and_modify(|v| v.push(message.clone()))
            .or_insert(vec![message]);

        Kvlm { dict }
    }

    pub fn serialize(&self) -> Bytes {
        let mut data = BytesMut::new();

        for (key, values) in self.dict.iter().filter(|(k, _)| *k != "message") {
            for value in values {
                data.extend_from_slice(key);
                data.put_u8(b' ');
                for byte in value {
                    data.put_u8(*byte);
                    if *byte == b'\n' {
                        data.put_u8(b' ');
                    }
                }
                data.put_u8(b'\n');
            }
        }

        data.put_u8(b'\n');

        // unwrap is safe because we have inserted "message" into dict
        data.extend_from_slice(&self.dict.get("message".as_bytes()).unwrap()[0]);

        data.into()
    }
}

impl Deref for Kvlm {
    type Target = IndexMap<Bytes, Vec<Bytes>>;

    fn deref(&self) -> &Self::Target {
        &self.dict
    }
}

impl DerefMut for Kvlm {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.dict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kvlm_parse() {
        let raw = Bytes::from_static(
            b"tree e02c1335b0dc9c63201c32e4325192291efe2ea4
parent 409f2bf19becc055a2bfb188bcced9d001842b23
author veronicaf <1204409815@qq.com> 1703757808 +0800
committer veronicaf <1204409815@qq.com> 1703757808 +0800
 123
 123

Hash-object and cat-file",
        );

        let kvlm = Kvlm::parse(&raw);

        assert_eq!(
            kvlm.get("tree".as_bytes()).unwrap(),
            &vec!["e02c1335b0dc9c63201c32e4325192291efe2ea4".as_bytes()]
        );
        assert_eq!(
            kvlm.get("parent".as_bytes()).unwrap(),
            &vec!["409f2bf19becc055a2bfb188bcced9d001842b23".as_bytes()]
        );

        assert_eq!(
            kvlm.get("author".as_bytes()).unwrap(),
            &vec!["veronicaf <1204409815@qq.com> 1703757808 +0800".as_bytes()]
        );
        assert_eq!(
            kvlm.get("committer".as_bytes()).unwrap(),
            &vec!["veronicaf <1204409815@qq.com> 1703757808 +0800\n123\n123".as_bytes()]
        );

        assert_eq!(
            kvlm.get("message".as_bytes()).unwrap(),
            &vec!["Hash-object and cat-file".as_bytes()]
        );

        assert_eq!(kvlm.serialize(), raw);
    }
}
