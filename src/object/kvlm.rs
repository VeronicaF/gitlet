use bytes::{BufMut, Bytes, BytesMut};
use indexmap::IndexMap;

pub struct Kvlm {
    pub dict: IndexMap<String, Vec<String>>,
}

impl Kvlm {
    pub fn new(dict: IndexMap<String, Vec<String>>) -> Kvlm {
        Kvlm { dict }
    }

    pub fn parse(raw: &Bytes) -> Kvlm {
        Kvlm {
            dict: kvlm_parse(raw),
        }
    }

    pub fn serialize(&self) -> Bytes {
        kvlm_serialize(self.dict.clone())
    }
}

enum KvlmState {
    Init,
    Key,
    Value,
    Message,
}

/// parse a key-value list with message
pub fn kvlm_parse(raw: &Bytes) -> IndexMap<String, Vec<String>> {
    let mut state = KvlmState::Init;
    let mut key = String::new();
    let mut value = String::new();
    let mut message = String::new();
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
                    key.push(byte as char);
                }
            }
            KvlmState::Key => {
                if byte == b' ' {
                    state = KvlmState::Value;
                } else {
                    key.push(byte as char);
                }
            }
            KvlmState::Value => {
                if byte == b'\n' {
                    if next_byte == Some(&b' ') {
                        // Continuation lines
                        value.push(b'\n' as char);
                        index += 1;
                    } else {
                        dict.entry(key.clone())
                            .and_modify(|v| v.push(value.clone()))
                            .or_insert(vec![value.clone()]);
                        key.clear();
                        value.clear();
                        state = KvlmState::Init;
                    }
                } else {
                    value.push(byte as char);
                }
            }
            KvlmState::Message => {
                message.push(byte as char);
            }
        }
        index += 1;
    }

    dict.entry("message".to_owned())
        .and_modify(|v| v.push(message.clone()))
        .or_insert(vec![message.clone()]);

    dict
}

pub fn kvlm_serialize(dict: IndexMap<String, Vec<String>>) -> Bytes {
    let mut data = BytesMut::new();

    for (key, values) in dict.iter().filter(|(k, _)| *k != "message") {
        for value in values {
            data.extend_from_slice(key.as_bytes());
            data.put_u8(b' ');
            data.extend_from_slice(value.replace('\n', "\n ").as_bytes());
            data.put_u8(b'\n');
        }
    }

    data.put_u8(b'\n');

    data.extend_from_slice(dict.get("message").unwrap()[0].as_bytes());

    data.into()
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

        let dict = kvlm_parse(&raw);

        assert_eq!(
            dict.get("tree").unwrap(),
            &vec!["e02c1335b0dc9c63201c32e4325192291efe2ea4".to_owned()]
        );
        assert_eq!(
            dict.get("parent").unwrap(),
            &vec!["409f2bf19becc055a2bfb188bcced9d001842b23".to_owned()]
        );

        assert_eq!(
            dict.get("author").unwrap(),
            &vec!["veronicaf <1204409815@qq.com> 1703757808 +0800".to_owned()]
        );
        assert_eq!(
            dict.get("committer").unwrap(),
            &vec!["veronicaf <1204409815@qq.com> 1703757808 +0800\n123\n123".to_owned()]
        );

        assert_eq!(
            dict.get("message").unwrap(),
            &vec!["Hash-object and cat-file".to_owned()]
        );

        assert_eq!(kvlm_serialize(dict), raw);
    }
}
