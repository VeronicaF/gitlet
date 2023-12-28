use crate::repository::Repository;
use anyhow::Context;
use std::path::PathBuf;

/// Compute path under repo's gitlet dir.
pub(crate) fn repo_path(repo: &Repository, path: &str) -> PathBuf {
    let path = PathBuf::from(path);

    if path.is_absolute() {
        path
    } else {
        repo.git_dir.join(path)
    }
}

pub(crate) fn repo_find(work_dir: impl Into<PathBuf>) -> Option<Repository> {
    let mut path = work_dir.into().canonicalize().ok()?;

    while !path.join(".gitlet").exists() {
        if !path.pop() {
            return None;
        }
    }

    Repository::load(path).ok()
}
