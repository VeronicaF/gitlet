/// # Branches
/// Branch is a reference to a commit.
///
/// Branches are mutable, and can be moved to point to different commits.
///
/// Branches are stored in the refs/heads/ directory in the git directory.
///
/// The current branch is stored in the HEAD file in the git directory.
///
/// # Detached Head
/// Detached head is when HEAD points directly to a commit, instead of a branch.
pub struct Branch {
    pub name: String,
    pub sha: String,
}
