mod branch;
pub mod tag;

use anyhow::Context;
use std::path::PathBuf;

/// Refs are text files containing a hexadecimal representation of an objects’s hash, encoded in ASCII.
///
/// Refs can also refer to another reference, and thus only indirectly to an objects.
pub fn resolve(path: &str) -> anyhow::Result<PathBuf> {
    let path = path.trim_end_matches('\n');
    if path.starts_with("ref: ") {
        let path = path.trim_start_matches("ref: ");
        let path = std::fs::read_to_string(path).context(format!("failed to read {}", path))?;
        resolve(&path)
    } else {
        Ok(PathBuf::from(path))
    }
}
