use anyhow::Context;
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path::PathBuf;

/// a gitlet repository
pub struct Repository {
    pub working_dir: PathBuf,
    pub git_dir: PathBuf,
    pub config: RepoConfig,
}

pub struct RepoConfig(ini::Ini);

impl RepoConfig {
    pub fn write_to_file(&self, path: impl Into<PathBuf>) -> anyhow::Result<()> {
        self.0
            .write_to_file(path.into())
            .context("failed to write config file")
    }

    pub fn load_from_file(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let config = ini::Ini::load_from_file(path.into()).context("failed to read config file")?;
        Ok(Self(config))
    }
}

impl Deref for RepoConfig {
    type Target = ini::Ini;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for RepoConfig {
    fn default() -> Self {
        let mut config = ini::Ini::new();
        config
            .with_section(Some("core".to_owned()))
            .set("repositoryformatversion", "0")
            .set("filemode", "false")
            .set("bare", "false");

        Self(config)
    }
}

impl Repository {
    /// Load a repository at path.
    pub fn load(working_dir: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let working_dir = working_dir.into();
        let git_dir = working_dir.join(".gitlet");

        anyhow::ensure!(
            git_dir.exists(),
            "not a gitlet repository (or any of the parent directories): {}",
            working_dir.display()
        );

        // Read configuration file in .git/config
        let config = RepoConfig::load_from_file(git_dir.join("config"))
            .context("failed to read config file")?;

        Ok(Self {
            working_dir,
            git_dir,
            config,
        })
    }

    /// Create a new repository at path.
    pub fn init(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let working_dir = path.into();
        let git_dir = working_dir.join(".gitlet");

        if git_dir.exists() {
            if !git_dir.is_dir() {
                anyhow::bail!(
                    "not a gitlet repository (or any of the parent directories): {}",
                    working_dir.display()
                );
            }

            if !git_dir.read_dir().iter().is_empty() {
                anyhow::bail!(
                    "gitlet repository has existing files: {}",
                    working_dir.display()
                );
            }
        } else {
            fs::create_dir_all(&git_dir).context("failed to create .gitlet directory")?;
            // create .gitlet directory
        }

        fs::create_dir_all(git_dir.join("objects"))
            .context("failed to create objects directory")?;
        fs::create_dir_all(git_dir.join("branches"))
            .context("failed to create branches directory")?;
        fs::create_dir_all(git_dir.join("refs/tags")).context("failed to create tags directory")?;
        fs::create_dir_all(git_dir.join("refs/heads"))
            .context("failed to create heads directory")?;

        fs::File::create(git_dir.join("description"))
            .context("failed to create description file")?
            .write_all(
                b"Unnamed repository; edit this file 'description' to name the repository.\n",
            )
            .context("failed to write description file")?;

        fs::File::create(git_dir.join("HEAD"))
            .context("failed to create HEAD file")?
            .write_all(b"ref: refs/heads/master\n")
            .context("failed to write HEAD file")?;

        fs::File::create(git_dir.join("config")).context("failed to create config file")?;

        let config = RepoConfig::default();
        config.write_to_file(git_dir.join("config"))?;

        Ok(Self {
            working_dir,
            git_dir,
            config,
        })
    }
}
