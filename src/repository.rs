use crate::objects::{GitObject, GitObjectTrait};
use anyhow::Context;
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
                anyhow::bail!("No gitlet repository found");
            }
        }

        Repository::load(path)
    }

    pub fn refs(&self) -> anyhow::Result<IndexMap<String, String>> {
        let refs_path = self.git_dir.join("refs");
        let prefix = PathBuf::from(&self.git_dir);

        let mut dict = IndexMap::new();

        for entry in walkdir::WalkDir::new(&refs_path) {
            let entry = entry.context(format!("failed to read entry: {}", refs_path.display()))?;
            if entry.file_type().is_dir() {
                continue;
            }

            let path = entry.path();
            let sha = self
                .resolve_ref(path)?
                .ok_or_else(|| anyhow::anyhow!("failed to resolve ref: {}", path.display()))?;

            let path = path
                .strip_prefix(&prefix)
                .unwrap() // this is safe because we know prefix is a parent of path
                .display()
                .to_string();

            dict.insert(path, sha);
        }

        Ok(dict)
    }

    /// Resolve a reference to an git object.
    ///
    /// Name can be a ref or a git object's sha
    ///
    /// If name is a ref, it will be resolved to a git object's sha using [Self::resolve_ref] first.
    /// Then if follow is true and the object is a tag object, it will be until a non-tag object is found.
    pub fn find_object(&self, name: &str, follow: bool) -> anyhow::Result<Option<String>> {
        let name = self.resolve_object(name)?;
        if !follow || name.is_none() {
            return Ok(name);
        }

        let mut depth = 0;

        // unwrap is safe because we have ensured that name is not None
        let mut name = name.unwrap();

        loop {
            anyhow::ensure!(depth < 10, "too many levels of symbolic references");

            // todo We read the whole object to get the header, which is not efficient.
            let object = GitObject::read_object(self, &name)?;

            if follow && object.header.fmt == crate::objects::Fmt::Tag {
                let tag_object = crate::objects::tag::Tag::from_bytes(object.data)?;
                name = tag_object
                    .object()
                    .context("tag object missing object field")?
                    .clone();
            } else {
                return Ok(Some(name));
            }
            depth += 1;
        }
    }

    /// resolve a reference to sha path
    ///
    /// The argument is a path to ref file, e.g. "refs/heads/master"
    ///
    /// returns None if the reference cannot be resolved
    // todo deal with recursive refs
    pub fn resolve_ref(&self, reference: impl Into<PathBuf>) -> anyhow::Result<Option<String>> {
        let path = self.git_dir.join(reference.into());

        // Sometimes, an indirect reference may be broken.  This is normal
        // in one specific case: we're looking for HEAD on a new repository
        // with no commits.  In that case, .git/HEAD points to "ref:
        // refs/heads/main", but .git/refs/heads/main doesn't exist yet
        // (since there's no commit for it to refer to).
        if !path.is_file() {
            return Ok(None);
        }

        let data = fs::read_to_string(&path)
            .context(format!("failed to read ref file: {}", path.display()))?;

        let data = data.trim_end_matches('\n');

        if data.starts_with("ref: ") {
            self.resolve_ref(&data[5..])
        } else {
            Ok(Some(data.to_string()))
        }
    }

    /// resolve a name to a git object's sha
    ///
    /// the name can be a "HEAD" literal, branch, tag, full sha, or short sha
    ///
    /// return None if the name cannot be resolved
    pub fn resolve_object(&self, name: &str) -> anyhow::Result<Option<String>> {
        let mut candidates = vec![];

        // case 1: name is HEAD literal
        if name == "HEAD" {
            // Head is a reference so we can use resolve_ref
            let head = self.resolve_ref("HEAD")?;
            if let Some(head) = head {
                candidates.push(head);
            }
        }

        // case 2: name is a short or full sha
        let hash_regex = regex::Regex::new(r"^[0-9a-f]{4,40}$").context("invalid regex")?;

        if hash_regex.is_match(name) {
            // name is a full or short sha
            let name = name.to_lowercase();
            let prefix = &name[..2];
            let path = &name[2..];

            let dir = self.git_dir.join("objects").join(prefix);

            anyhow::ensure!(dir.exists(), "object not found: {}", name);

            // filter out non-files and non-matching files
            let entries = walkdir::WalkDir::new(dir).into_iter().filter(|e| {
                e.as_ref().is_ok_and(|e| {
                    e.file_type().is_file()
                        && e.file_name()
                            .to_str()
                            .map(|s| s.starts_with(path))
                            .unwrap_or(false)
                })
            });

            for entry in entries {
                let entry = entry.context("failed to read entry")?;
                let file_name = entry.file_name().to_str().context("invalid file name")?;
                candidates.push(prefix.to_string() + file_name);
            }
        }

        // case 3: name is a tag or branch

        let maybe_tag = self.resolve_ref(format!("refs/tags/{}", name))?;
        if let Some(tag) = maybe_tag {
            candidates.push(tag);
        }

        let maybe_branch = self.resolve_ref(format!("refs/heads/{}", name))?;
        if let Some(branch) = maybe_branch {
            candidates.push(branch);
        }

        anyhow::ensure!(candidates.len() <= 1, "ambiguous object name: {}", name,);

        Ok(if candidates.is_empty() {
            None
        } else {
            // unwrap is safe because we have ensured that candidates is not empty
            Some(candidates.pop().unwrap())
        })
    }
}
