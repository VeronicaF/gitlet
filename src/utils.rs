use crate::repository::Repository;
use anyhow::bail;
use sha1::Digest;
use std::path::PathBuf;

pub fn sha(data: &[u8]) -> String {
    let mut hasher = sha1::Sha1::new();

    hasher.update(data);

    hex::encode(hasher.finalize())
}

pub fn repo_find(work_dir: impl Into<PathBuf>) -> anyhow::Result<Repository> {
    let mut path = work_dir.into().canonicalize()?;

    while !path.join(".gitlet").exists() {
        if !path.pop() {
            bail!("No gitlet repository found");
        }
    }

    Repository::load(path)
}
