use crate::ignore::GitIgnore;
use crate::index::Index;
use crate::objects::tree::{Tree, TreeEntry};
use crate::objects::{Fmt, GitObject, GitObjectTrait};
use crate::utils::sha;
use anyhow::Context;
use bytes::Bytes;
use indexmap::{IndexMap, IndexSet};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::ops::Deref;
use std::os::macos::fs::MetadataExt;
use std::path::PathBuf;

/// a gitlet repository
pub struct Repository {
    pub work_tree: PathBuf,
    pub git_dir: PathBuf,
    pub config: RepoConfig,
}

#[derive(Debug)]
pub struct RepoConfig(configparser::ini::Ini);

impl RepoConfig {
    pub fn user(&self) -> Option<String> {
        let name = self.get("user", "name")?;
        let email = self.get("user", "email")?;

        Some(format!("{} <{}>", name, email))
    }
}

impl Deref for RepoConfig {
    type Target = configparser::ini::Ini;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for RepoConfig {
    fn default() -> Self {
        let mut config = configparser::ini::Ini::new();

        config.setstr("core", "repositoryformatversion", Some("0"));
        config.setstr("core", "filemode", Some("false"));
        config.setstr("core", "bare", Some("false"));

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
        let mut config = configparser::ini::Ini::new();

        config
            .load(git_dir.join("config"))
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(Self {
            work_tree: working_dir,
            git_dir,
            config: RepoConfig(config),
        })
    }

    /// Create a new repository at path.
    pub fn init(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let work_tree = path.into();
        let git_dir = work_tree.join(".gitlet");

        if git_dir.exists() {
            if !git_dir.is_dir() {
                anyhow::bail!(
                    "not a gitlet repository (or any of the parent directories): {}",
                    work_tree.display()
                );
            }

            if !git_dir.read_dir().iter().is_empty() {
                anyhow::bail!(
                    "gitlet repository has existing files: {}",
                    work_tree.display()
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
        config.write(git_dir.join("config"))?;

        Ok(Self {
            work_tree,
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
            let object = self.read_object(&name)?;

            if follow && object.header.fmt == Fmt::Tag {
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

    pub fn read_object(&self, sha: &str) -> anyhow::Result<GitObject> {
        let path = self.git_dir.join("objects").join(&sha[..2]).join(&sha[2..]);

        anyhow::ensure!(path.exists(), "objects not found: {}", sha);

        let file = fs::File::open(&path)?;

        let mut data = Vec::new();
        flate2::bufread::ZlibDecoder::new_with_decompress(
            std::io::BufReader::new(file),
            flate2::Decompress::new(true),
        )
        .read_to_end(&mut data)
        .context("failed to read zlib data")?;

        let data = Bytes::from(data);

        GitObject::from_bytes(data)
    }

    /// write objects to disk
    ///
    /// returns sha of objects
    pub fn write_object(&self, object: &GitObject) -> anyhow::Result<String> {
        let data = object.serialize()?;

        let sha = sha(&data);

        let path = self.git_dir.join("objects").join(&sha[..2]).join(&sha[2..]);

        if path.exists() {
            return Ok(sha);
        }

        fs::create_dir_all(
            path.parent()
                .context(format!("failed to get path parent: {}", path.display()))?,
        )?;

        let file = fs::File::create(&path)?;

        let mut encoder = flate2::write::ZlibEncoder::new(file, flate2::Compression::default());

        encoder
            .write_all(&data)
            .context("failed to write zlib data")?;

        encoder.finish().context("failed to write zlib data")?;

        Ok(sha)
    }

    pub fn read_index(&self) -> anyhow::Result<Index> {
        let index_path = self.git_dir.join("index");

        // New repositories have no index!
        if !index_path.exists() {
            return Ok(Index::default());
        }

        let data = fs::read(&index_path).context("failed to read index file")?;

        let data = Bytes::from(data);

        Index::from_bytes(data)
    }

    pub fn write_index(&self, index: &Index) -> anyhow::Result<()> {
        let index_path = self.git_dir.join("index");

        let data = index.serialize()?;

        fs::write(index_path, data).context("failed to write index file")?;

        Ok(())
    }

    pub fn read_ignore(&self) -> anyhow::Result<GitIgnore> {
        let mut ignore = GitIgnore::default();

        // Read local configuration in .git/info/exclude
        let exclude_path = self.git_dir.join("info").join("exclude");

        if exclude_path.exists() {
            let data = fs::read_to_string(&exclude_path).context("failed to read exclude file")?;
            let rules = GitIgnore::parse(&data);
            ignore.global.push(rules);
        }

        // Global configuration
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .context("failed to read env")
            .ok()
            .unwrap_or("~/.config".to_string());

        let config_home = PathBuf::from(config_home);
        let global_ignore_path = config_home.join("gitlet").join("ignore");

        if global_ignore_path.exists() {
            let data = fs::read_to_string(&global_ignore_path)
                .context("failed to read global ignore file")?;
            let rules = GitIgnore::parse(&data);
            ignore.global.push(rules);
        }

        // .gitignore files in the index

        let index = self.read_index()?;
        for entry in index
            .entries
            .iter()
            .filter(|e| e.name == ".gitignore" || e.name.ends_with("/.gitignore"))
        {
            let dirname = PathBuf::from(&entry.name)
                .parent()
                .context("invalid path")?
                .to_str()
                .context("invalid path")?
                .to_owned();

            let object = self.read_object(&entry.sha)?;

            let lines = String::from_utf8_lossy(&object.data).to_string();

            let rules = GitIgnore::parse(&lines);

            ignore.local.insert(dirname, rules);
        }

        Ok(ignore)
    }

    pub fn active_branch(&self) -> anyhow::Result<String> {
        let head =
            fs::read_to_string(self.git_dir.join("HEAD")).context("failed to read HEAD file")?;
        let head = head.trim();
        if head.starts_with("ref: refs/heads/") {
            Ok(head.trim_start_matches("ref: refs/heads/").to_string())
        } else {
            anyhow::bail!("Detached HEAD found: {}", head);
        }
    }

    /// Create a tree from index object.
    ///
    /// Returns the sha of the root tree object.
    ///
    /// Notice: this function will write tree objects to the disk.
    fn create_tree_from_index(&self, index: &Index) -> anyhow::Result<String> {
        enum T<'a> {
            IndexEntry(&'a crate::index::IndexEntry), // file in a dictionary
            TreeInfo((String, String)),               // file name, sha; dictionary in a dictionary
        }

        let mut map = HashMap::new();

        // collect entries by parent path
        for entry in &index.entries {
            let path = &entry.name;
            let path_buf = PathBuf::from(path);
            let mut parent = path_buf
                .parent()
                .context(format!("invalid path: {}", path))?
                .to_owned();
            let parent_str = parent.to_str().context("invalid path")?.to_string();

            while parent != PathBuf::from("") {
                let parent_str = parent.to_str().context("invalid path")?;

                map.entry(parent_str.to_string()).or_insert(vec![]);
                parent.pop();
            }
            map.entry(parent_str.to_string())
                .or_insert(vec![])
                .push(T::IndexEntry(entry));
        }

        let mut sha1 = String::new();

        // sort paths by length so we can create tree objects from bottom to top
        let mut paths: Vec<_> = map.keys().cloned().collect();

        paths.sort_by_key(|a| !a.len());

        for path in paths {
            let mut tree = Tree::default();

            // Safe: unwrap is safe because we have ensured that key is in map
            let entries = map.get(&path).unwrap();

            for entry in entries {
                let tree_entry = match entry {
                    T::IndexEntry(index_entry) => {
                        let path = PathBuf::from(&index_entry.name);

                        let file_name = path.file_name().context("invalid path")?;

                        let file_name = PathBuf::from(file_name);

                        TreeEntry::try_new(
                            format!(
                                "{:0>2o}{:0>4o}",
                                index_entry.mode_type, index_entry.mode_perms
                            ),
                            file_name,
                            index_entry.sha.clone(),
                        )?
                    }
                    T::TreeInfo((file_name, sha1)) => TreeEntry::try_new(
                        "40000".to_string(),
                        PathBuf::from(file_name),
                        sha1.clone(),
                    )?,
                };
                tree.0.push(tree_entry);
            }

            let tree_object = GitObject::new(Fmt::Tree, tree.serialize()?);

            sha1 = self.write_object(&tree_object)?;

            if path.is_empty() {
                break;
            }

            let path_buf = PathBuf::from(path);
            let parent = path_buf.parent().unwrap().to_str().unwrap().to_string();
            let file_name = path_buf.file_name().unwrap().to_str().unwrap().to_string();
            map.entry(parent)
                .or_insert(vec![])
                .push(T::TreeInfo((file_name, sha1.clone())));
        }

        Ok(sha1)
    }
}

impl Repository {
    /// rm files from index
    pub fn rm(
        &self,
        paths: &Vec<String>,
        delete_file: bool,
        ignore_missing: bool,
    ) -> anyhow::Result<Index> {
        let mut index = self.read_index()?;
        let mut abs_paths = IndexSet::with_capacity(paths.len());

        for path in paths {
            let path = PathBuf::from(path).canonicalize().context("invalid path")?;
            if path.starts_with(&self.work_tree) {
                abs_paths.insert(path);
            } else {
                anyhow::bail!("path not in working directory: {}", path.display());
            }
        }

        let (remove, kept): (Vec<_>, Vec<_>) = index.entries.into_iter().partition(|path| {
            let abs_path = self.work_tree.join(&path.name);
            if abs_paths.contains(&abs_path) {
                abs_paths.remove(&abs_path);
                true
            } else {
                false
            }
        });

        if !ignore_missing && !abs_paths.is_empty() {
            anyhow::bail!(
                "path not in index: {}",
                // unwrap is safe because we have ensured that abs_paths is not empty
                abs_paths.iter().next().unwrap().display()
            );
        }

        if delete_file {
            for e in remove {
                fs::remove_file(&e.name).context(format!("failed to remove file: {}", e.name))?;
            }
        }

        index.entries = kept;

        self.write_index(&index)?;

        Ok(index)
    }

    pub fn add(&self, paths: &Vec<String>) -> anyhow::Result<()> {
        // rm ensures that paths are in working directory
        let mut index = self.rm(paths, false, true)?;

        for path in paths {
            let abs_path = PathBuf::from(path).canonicalize().context("invalid path")?;

            let object = GitObject::from_file(&abs_path, Fmt::Blob)?;

            let sha = self.write_object(&object)?;

            let metadata = abs_path.metadata().context("failed to read metadata")?;

            let ctime_s = metadata.st_ctime() as u32;
            let ctime_ns = (metadata.st_ctime_nsec() % 1_000_000_000) as u32;

            let mtime_s = metadata.st_mtime() as u32;
            let mtime_ns = (metadata.st_mtime_nsec() % 1_000_000_000) as u32;

            let index_entry = crate::index::IndexEntry {
                name: abs_path
                    .strip_prefix(&self.work_tree)
                    .unwrap() // unwrap is safe because we have ensured that abs_path is a child of work_tree
                    .to_str()
                    .unwrap()
                    .to_owned(),
                ctime: (ctime_s, ctime_ns),
                mtime: (mtime_s, mtime_ns),
                dev: metadata.st_dev() as u32,
                ino: metadata.st_ino() as u32,
                mode_type: 0b1000,
                mode_perms: 0o644,
                uid: metadata.st_uid(),
                gid: metadata.st_gid(),
                fsize: metadata.st_size() as u32,
                sha,
                flag_assume_valid: false,
                flag_stage: 0,
            };

            index.entries.push(index_entry);
        }

        self.write_index(&index)?;

        Ok(())
    }

    pub fn read_config(&self) -> anyhow::Result<RepoConfig> {
        let mut config = configparser::ini::Ini::new();

        let user_home = dirs::home_dir().context("failed to get home directory")?;

        let config_dir = if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(xdg_config_home)
        } else {
            user_home.join(".config")
        };

        let config_files = [
            config_dir.join("git/config"),
            user_home.join(".gitconfig"),
            self.git_dir.join("config"),
        ];

        for config_file in config_files {
            if config_file.exists() {
                let config_file = config_file.canonicalize().context("invalid path")?;

                config
                    .load_and_append(config_file)
                    .map_err(|e| anyhow::anyhow!(e))?;
            }
        }

        Ok(RepoConfig(config))
    }

    pub fn commit(&self, message: String) -> anyhow::Result<String> {
        let index = self.read_index()?;

        // create tree object and write it to disk from index file
        let tree_sha = self.create_tree_from_index(&index)?;

        let parent = self.resolve_ref("HEAD")?;

        let config = self.read_config()?;

        // create commit object and write it to disk
        let commit = crate::objects::commit::Commit::new(
            tree_sha,
            parent,
            config.user().context("failed to get user")?,
            chrono::Local::now(),
            message,
        );

        let commit_sha = self.write_object(&GitObject::new(Fmt::Commit, commit.serialize()?))?;

        // Update HEAD so our commit is now the tip of the active branch.

        if let Ok(active_branch) = self.active_branch() {
            // If we're on a branch, we update refs/heads/BRANCH
            let branch_path = self.git_dir.join("refs").join("heads").join(active_branch);
            fs::write(branch_path, format!("{}\n", commit_sha))
                .context("failed to write branch file")?;
        } else {
            // Otherwise, we update HEAD directly
            fs::write(self.git_dir.join("HEAD"), format!("{}\n", commit_sha))
                .context("failed to write HEAD file")?;
        }

        Ok(commit_sha)
    }
}
