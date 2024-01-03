use sha1::Digest;

pub fn sha(data: &[u8]) -> String {
    let mut hasher = sha1::Sha1::new();

    hasher.update(data);

    hex::encode(hasher.finalize())
}
