use anyhow::{bail, Context};
use indexmap::IndexMap;
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

    pub fn find(work_dir: impl Into<PathBuf>) -> anyhow::Result<Repository> {
        let mut path = work_dir.into().canonicalize()?;

        while !path.join(".gitlet").exists() {
            if !path.pop() {
                bail!("No gitlet repository found");
            }
        }

        Repository::load(path)
    }

    pub fn refs(&self) -> anyhow::Result<IndexMap<String, String>> {
        let path = self.git_dir.join("refs");
        let prefix = PathBuf::from(&self.git_dir);

        fn run(
            path: PathBuf,
            prefix: &PathBuf,
            dict: &mut IndexMap<String, String>,
        ) -> anyhow::Result<()> {
            let dir = path
                .read_dir()
                .context(format!("failed to read dir: {}", path.display()))?;

            for entry in dir {
                let entry = entry.context(format!("failed to read entry: {}", path.display()))?;
                let path = entry.path();

                if path.is_dir() {
                    run(path, prefix, dict)?;
                } else {
                    let sha = fs::read_to_string(&path)
                        .context(format!("failed to read ref file: {}", path.display()))?;

                    let sha = crate::object::reference::Ref::resolve(&sha)?
                        .to_str()
                        .context(format!("failed to convert ref to str: {}", path.display()))?
                        .to_string();

                    let path = path
                        .strip_prefix(prefix)
                        .unwrap() // this is safe because we know prefix is a parent of path
                        .display()
                        .to_string();

                    dict.insert(path, sha);
                }
            }

            Ok(())
        }

        let mut dict = IndexMap::new();

        run(path, &prefix, &mut dict)?;

        Ok(dict)
    }
}
