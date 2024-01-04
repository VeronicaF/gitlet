use bytes::{BufMut, Bytes, BytesMut};
use indexmap::IndexMap;
use std::ops::{Deref, DerefMut};

/// This is used to parse and store a key-value list with message
/// used in git objects commit and tag
pub struct Kvlm {
    pub dict: IndexMap<String, Vec<String>>,
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
    pub fn parse(raw: Bytes) -> anyhow::Result<Self> {
        #[derive(Debug, PartialEq)]
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
        let mut dict = IndexMap::<String, Vec<String>>::new();

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
                            let key = String::from_utf8_lossy(&key).to_string();
                            let value = value.split().freeze();
                            let value = String::from_utf8_lossy(&value).to_string();
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
        let message = String::from_utf8_lossy(&message).to_string();

        dict.entry("message".to_string())
            .and_modify(|v| v.push(message.clone()))
            .or_insert(vec![message]);

        anyhow::ensure!(state == KvlmState::Message, "invalid kvlm");

        Ok(Kvlm { dict })
    }

    pub fn serialize(&self) -> Bytes {
        let mut data = BytesMut::new();

        for (key, values) in self.dict.iter().filter(|(k, _)| **k != "message") {
            for value in values {
                data.extend_from_slice(key.as_bytes());
                data.put_u8(b' ');
                for byte in value.as_bytes() {
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
        let message = self.dict.get("message").unwrap()[0].as_bytes();

        data.extend_from_slice(message);

        data.into()
    }

    /// get a single value of a key
    ///
    /// returns None if the key does not exist or the key has multiple values
    pub fn get_single(&self, key: &str) -> Option<&String> {
        let values = self.dict.get(key)?;
        if values.len() != 1 {
            return None;
        }
        values.first()
    }

    pub fn get(&self, key: &str) -> Option<&Vec<String>> {
        self.dict.get(key)
    }
}

impl Deref for Kvlm {
    type Target = IndexMap<String, Vec<String>>;

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

Hash-objects and cat-file",
        );

        let kvlm = Kvlm::parse(raw.clone()).unwrap();

        assert_eq!(
            kvlm.get("tree").unwrap(),
            &vec!["e02c1335b0dc9c63201c32e4325192291efe2ea4"]
        );
        assert_eq!(
            kvlm.get("parent").unwrap(),
            &vec!["409f2bf19becc055a2bfb188bcced9d001842b23"]
        );

        assert_eq!(
            kvlm.get("author").unwrap(),
            &vec!["veronicaf <1204409815@qq.com> 1703757808 +0800"]
        );
        assert_eq!(
            kvlm.get("committer").unwrap(),
            &vec!["veronicaf <1204409815@qq.com> 1703757808 +0800\n123\n123"]
        );

        assert_eq!(
            kvlm.get("message").unwrap(),
            &vec!["Hash-objects and cat-file"]
        );

        assert_eq!(kvlm.serialize(), raw);
    }
}
