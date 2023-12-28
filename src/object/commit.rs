pub struct Commit {
    tree: String,
    parent: Option<String>,
    author: String,
    committer: String,
    pgp_signature: String,
}
