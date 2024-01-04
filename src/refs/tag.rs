use crate::repository::Repository;

pub struct Tag {
    tag: String,
    object: String,
}

impl Tag {
    pub fn new(tag: String, object: String) -> Self {
        Self { tag, object }
    }

    pub fn read_from(repo: &Repository, tag: String) -> anyhow::Result<Self> {
        let tag_path = repo.git_dir.join("refs").join("tags").join(&tag);
        anyhow::ensure!(tag_path.exists(), "tag {} not found", &tag);

        let sha = std::fs::read_to_string(tag_path)?;

        Ok(Self::new(tag, sha))
    }

    pub fn write_to(&self, repo: &Repository) -> anyhow::Result<()> {
        let tag_path = repo.git_dir.join("refs").join("tags").join(&self.tag);
        std::fs::write(tag_path, self.object.as_bytes())?;
        Ok(())
    }
}
