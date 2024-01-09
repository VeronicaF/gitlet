use anyhow::{ensure, Context};
use clap::{Parser, Subcommand};
use gitlet::objects::{Fmt, GitObject, GitObjectTrait};
use gitlet::repository::Repository;
use indexmap::{IndexMap, IndexSet};
use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// init gitlet repository
    Init {
        /// path to create repository in
        #[arg(help = "Initialize a new, empty repository.", default_value = ".")]
        path: PathBuf,
    },
    /// Provide content of repository objects
    CatFile {
        /// type
        #[arg(
            value_enum,
            value_name = "type",
            help = "Specify the expected type.",
            default_value = "blob",
            required = true
        )]
        fmt: Fmt,
        /// file to cat
        #[arg(help = "The objects to display.")]
        object: String,
    },

    /// Compute objects ID and optionally creates a blob from a file
    HashObject {
        /// Actually write the objects into the database
        #[arg(short)]
        write: bool,
        #[arg(
            value_enum,
            short = 't',
            value_name = "type",
            help = "Specify the expected type.",
            default_value = "blob"
        )]
        fmt: Fmt,
        /// Read objects from <file>
        path: PathBuf,
    },

    /// Display history of a given commit.
    Log {
        /// Commit to start at
        #[arg(default_value = "HEAD")]
        commit: String,
    },
    /// List the contents of a tree objects
    LsTree {
        /// Recurse into sub-trees
        #[arg(short)]
        recursive: bool,
        /// A tree-ish objects.
        tree: String,
    },

    /// Checkout a commit inside of a directory.
    /// todo this just clones file by tree into the directory, does not update HEAD
    Checkout {
        /// The commit or tree or ref to checkout.
        name: String,
        /// The EMPTY directory to checkout on.
        path: PathBuf,
    },
    /// List all refs in a local repository
    ShowRef,
    /// tag
    Tag {
        /// Whether to create a tag objects
        #[arg(short = 'a', requires = "name")]
        create_tag_object: bool,
        /// The new tag's name.
        name: Option<String>,
        /// The objects the new tag will point to
        #[arg(default_value = "HEAD")]
        object: String,
    },
    /// List all the stage files
    LsFiles {
        /// Show everything
        #[arg(long, short)]
        verbose: bool,
    },
    /// Check path(s) against ignore rules.
    CheckIgnore {
        /// Paths to check
        #[arg(required = true)]
        path: Vec<String>,
    },
    /// Show the working tree status.
    Status,
    /// Remove files from the working tree and the index.
    Rm {
        /// Files to remove
        path: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { path } => {
            let repo = Repository::init(path)?;
            println!("init at path: {}", repo.git_dir.display());
        }
        Commands::CatFile { fmt, object } => {
            let repo = Repository::find(".")?;
            let object = repo
                .find_object(&object, true)?
                .ok_or(anyhow::anyhow!("object not found: {}", object))?;

            let object = GitObject::read_object(&repo, &object)?;

            ensure!(object.header.fmt == fmt, "objects type mismatch");

            println!("{}", object);
        }
        Commands::HashObject { write, fmt, path } => {
            let repo = Repository::find(".")?;
            anyhow::ensure!(path.exists(), "file does not exist: {}", path.display());

            let data = std::fs::read(&path)?;

            let object = GitObject::new(fmt, data);

            let sha = if write {
                object.write_object(&repo)?
            } else {
                gitlet::utils::sha(&object.serialize())
            };

            println!("{}", sha);
        }
        Commands::Log { commit } => {
            let repo = Repository::find(".")?;
            let commit = repo
                .find_object(&commit, true)?
                .ok_or(anyhow::anyhow!("object not found: {}", commit))?;

            // todo do not clone
            fn log_graphviz(
                repo: &Repository,
                sha: &str,
                visited: &mut BTreeSet<String>,
            ) -> anyhow::Result<()> {
                if visited.contains(sha) {
                    return Ok(());
                }

                visited.insert(sha.to_string());

                let commit = GitObject::read_object(repo, sha)?;

                anyhow::ensure!(commit.header.fmt == Fmt::Commit, "objects type mismatch");

                let commit = gitlet::objects::commit::Commit::from_bytes(commit.data)?;
                let short_sha = &sha[..8];

                let mut message = commit
                    .message()
                    .unwrap_or(&"".to_owned())
                    .replace('\\', "\\\\")
                    .replace('\"', "\\\"");

                if let Some(i) = message.find('\n') {
                    message = message[..i].to_owned();
                }

                print!("  c_{} [label=\"{}: {}\"]", sha, short_sha, message);

                if let Some(parents) = commit.parents() {
                    for parent in parents {
                        print!("  c_{} -> c_{}", sha, parent);
                        log_graphviz(repo, parent, visited)?;
                    }
                }

                Ok(())
            }

            print!(r"digraph log{{");
            print!("  node[shape=rect]");
            log_graphviz(&repo, &commit, &mut BTreeSet::new())?;
            println!("}}");
        }
        Commands::LsTree { recursive, tree } => {
            let repo = Repository::find(".")?;

            fn ls_tree(
                repo: &Repository,
                recursive: bool,
                name: &str,
                prefix: PathBuf,
            ) -> anyhow::Result<()> {
                let name = repo
                    .find_object(name, true)?
                    .ok_or(anyhow::anyhow!("object not found: {}", name))?;

                let object = GitObject::read_object(repo, &name)?;

                // if name refers to a commit, we need to get the tree
                if object.header.fmt == Fmt::Commit {
                    let commit = gitlet::objects::commit::Commit::from_bytes(object.data)?;
                    let tree = commit.tree().ok_or(anyhow::anyhow!("commit has no tree"))?;
                    ls_tree(repo, recursive, tree, prefix)?;
                    return Ok(());
                }

                let tree_object = object;

                ensure!(tree_object.header.fmt == Fmt::Tree, "objects type mismatch");

                let tree = gitlet::objects::tree::Tree::from_bytes(tree_object.data)?;

                for (mode, path, sha1) in tree.0 {
                    let file_type = mode.file_type()?;
                    let mode = mode.0;
                    let sha1_str = sha1.0;
                    if recursive && file_type == gitlet::objects::tree::FileType::Tree {
                        ls_tree(repo, recursive, &sha1_str, prefix.join(path))?;
                    } else {
                        println!(
                            "{} {} {}\t{}",
                            mode,
                            file_type.to_str(),
                            sha1_str,
                            prefix.join(&path).display()
                        );
                    }
                }

                Ok(())
            }

            ls_tree(&repo, recursive, &tree, PathBuf::from(""))?;
        }
        Commands::Checkout { name, path } => {
            let repo = Repository::find(".")?;

            let name = repo
                .find_object(&name, true)?
                .ok_or(anyhow::anyhow!("object not found: {}", name))?;

            let commit = GitObject::read_object(&repo, &name)?;

            ensure!(
                commit.header.fmt == Fmt::Commit,
                "objects type mismatch, expected commit"
            );

            let commit = gitlet::objects::commit::Commit::from_bytes(commit.data)?;

            let tree = commit.tree().ok_or(anyhow::anyhow!("commit has no tree"))?;
            if path.exists() {
                ensure!(path.is_dir(), "path is not a directory: {}", path.display());
                ensure!(
                    path.read_dir()?.next().is_none(),
                    "path is not empty: {}",
                    path.display()
                );
            } else {
                std::fs::create_dir_all(&path)?;
            }

            fn checkout(repo: &Repository, tree: &str, prefix: PathBuf) -> anyhow::Result<()> {
                let tree_object = GitObject::read_object(repo, tree)?;
                ensure!(
                    tree_object.header.fmt == Fmt::Tree,
                    "objects type mismatch, expected tree"
                );
                let tree = gitlet::objects::tree::Tree::from_bytes(tree_object.data)?;

                for (mode, path, sha1) in tree.0 {
                    let object = GitObject::read_object(repo, &sha1.0)?;
                    let dest = prefix.join(&path);

                    let file_type = mode.file_type()?;

                    match file_type {
                        gitlet::objects::tree::FileType::Tree => {
                            std::fs::create_dir_all(&dest)?;
                            checkout(repo, &sha1.0, dest)?;
                        }
                        gitlet::objects::tree::FileType::Blob => {
                            std::fs::write(&dest, object.data)?;
                        }
                        gitlet::objects::tree::FileType::SymLink => {
                            unimplemented!()
                        }
                        gitlet::objects::tree::FileType::Commit => {
                            unimplemented!()
                        }
                    }
                }

                Ok(())
            }

            checkout(&repo, tree, path)?;
        }
        Commands::ShowRef => {
            let repo = Repository::find(".")?;

            let refs = repo.refs()?;

            for (path, sha) in refs {
                println!("{} {}", sha, path);
            }
        }
        Commands::Tag {
            name,
            create_tag_object,
            object,
        } => {
            let repo = Repository::find(".")?;

            // create a tag
            if let Some(name) = name {
                let mut sha = repo
                    .find_object(&object, true)?
                    .ok_or(anyhow::anyhow!("object not found: {}", object))?;

                // create tag
                if create_tag_object {
                    let tag_object = gitlet::objects::tag::Tag::new(
                        name.clone(),
                        sha.clone(),
                        "default@default.com".to_owned(),
                        "A tag generated by gitlet, which won't let you customize the message!"
                            .to_owned(),
                    );

                    let bytes = tag_object.serialize()?;

                    let git_object = GitObject::new(Fmt::Tag, bytes.into());

                    sha = git_object.write_object(&repo)?;
                }

                let tag_ref = gitlet::refs::tag::Tag::new(name, sha);

                tag_ref.write_to(&repo)?;
            } else {
                // list tags
                let tags_path = repo.git_dir.join("refs").join("tags");
                for entry in walkdir::WalkDir::new(tags_path) {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file() {
                        let sha = std::fs::read_to_string(path)?;
                        let sha = sha.trim_end_matches('\n');
                        println!("{} {}", sha, entry.file_name().to_string_lossy());
                    }
                }
            }
        }
        Commands::LsFiles { verbose } => {
            let repo = Repository::find(".")?;

            let index = repo.read_index()?;

            if verbose {
                println!(
                    "Index file format v{}, containing {} entries.",
                    index.version,
                    index.entries.len()
                )
            }

            for e in index.entries {
                println!("{}", e.name);
                if verbose {
                    println!("  {} with perms: {:o}", e.mode_type_str(), e.mode_perms);
                    println!("  on blob: {}", e.sha);

                    let ctime = chrono::DateTime::<chrono::Utc>::from_timestamp(
                        e.ctime.0 as i64,
                        e.ctime.1,
                    )
                    .context("invalid ctime")?;
                    let mtime = chrono::DateTime::<chrono::Utc>::from_timestamp(
                        e.mtime.0 as i64,
                        e.mtime.1,
                    )
                    .context("invalid mtime")?;
                    println!("  created: {}, modified: {}", ctime, mtime);
                    println!("  device: {}, inode: {}", e.dev, e.ino);
                    let user = users::get_user_by_uid(e.uid).context("invalid uid")?;
                    let group = users::get_group_by_gid(e.gid).context("invalid gid")?;
                    println!(
                        "  user: {} ({})  group: {} ({})",
                        user.name().to_string_lossy(),
                        e.uid,
                        group.name().to_string_lossy(),
                        e.gid
                    );
                    println!(
                        "  flags: stage={} assume_valid={}",
                        e.flag_stage, e.flag_assume_valid
                    )
                }
            }
        }
        Commands::CheckIgnore { path } => {
            let repo = Repository::find(".")?;

            let ignore = repo.read_ignore()?;

            for p in path {
                let result = ignore.check(&p)?;
                if let Some(true) = result {
                    println!("{}: ignored", p);
                } else {
                    println!("{}: not ignored", p);
                }
            }
        }
        Commands::Status => {
            let repo = Repository::find(".")?;
            let index = repo.read_index()?;

            // part 1: current branch
            if let Ok(branch) = repo.active_branch() {
                println!("On branch {}.", branch);
            } else {
                println!(
                    "HEAD detached at {}",
                    repo.find_object("HEAD", true)?.context("HEAD not found")?
                );
            }

            // part 2: changes staged for commit
            // index contains the staged files
            // head contains last commit files
            fn tree_to_dict(
                repo: &Repository,
                tree: &str,
                prefix: &PathBuf,
                dict: &mut IndexMap<String, String>,
            ) -> anyhow::Result<()> {
                let tree_or_commit = repo
                    .find_object(tree, true)?
                    .ok_or(anyhow::anyhow!("object not found: {}", tree))?;

                let object = GitObject::read_object(repo, &tree_or_commit)?;

                if let Fmt::Commit = object.header.fmt {
                    let commit = gitlet::objects::commit::Commit::from_bytes(object.data.clone())?;
                    let tree = commit.tree().ok_or(anyhow::anyhow!("commit has no tree"))?;
                    return tree_to_dict(repo, tree, prefix, dict);
                }

                ensure!(
                    object.header.fmt == Fmt::Tree,
                    "objects type mismatch, expected tree"
                );

                let tree_object = object;

                let tree = gitlet::objects::tree::Tree::from_bytes(tree_object.data)?;

                for (mode, path, sha1) in tree.0 {
                    let dest = path;
                    let file_type = mode.file_type()?;

                    match file_type {
                        gitlet::objects::tree::FileType::Tree => {
                            tree_to_dict(repo, &sha1.0, &prefix.join(dest), dict)?;
                        }
                        gitlet::objects::tree::FileType::Blob => {
                            dict.insert(prefix.join(dest).display().to_string(), sha1.0);
                        }
                        gitlet::objects::tree::FileType::SymLink => {
                            unimplemented!()
                        }
                        gitlet::objects::tree::FileType::Commit => {
                            unimplemented!()
                        }
                    }
                }

                Ok(())
            }

            let mut head = IndexMap::new();

            // transform the tree into a dict<path, sha1>
            tree_to_dict(&repo, "HEAD", &PathBuf::from(""), &mut head)?;

            println!("Changes to be committed:");
            // then compare with the index
            for entry in &index.entries {
                if let Some(sha) = head.get(&entry.name) {
                    if sha != &entry.sha {
                        println!("  modified: {}", entry.name);
                    }
                    head.remove(&entry.name);
                } else {
                    println!("  added:   {}", entry.name);
                }
            }

            for (name, _) in head {
                println!("  deleted: {}", name);
            }

            // part 3: changes not staged for commit
            println!("Changes not staged for commit:");

            let ignore = repo.read_ignore()?;

            let mut all_files = IndexSet::new();

            for entry in walkdir::WalkDir::new(&repo.work_tree) {
                let entry = entry.context("failed to read entry")?;

                let path = entry.path();

                if (path.is_dir() || path.starts_with(&repo.git_dir))
                    || (path.starts_with(repo.git_dir.with_file_name(".git")))
                {
                    continue;
                }

                all_files.insert(path.to_owned());
            }

            for entry in &index.entries {
                let abs_path = repo.work_tree.join(&entry.name);

                if !abs_path.exists() {
                    println!("  deleted: {}", entry.name);
                } else {
                    let meta = abs_path.metadata()?;

                    // Compare metadata
                    let ctime_ns = entry.ctime.0 as i64 * 1_000_000_000 + entry.ctime.1 as i64;
                    let mtime_ns = entry.mtime.0 as i64 * 1_000_000_000 + entry.mtime.1 as i64;

                    // todo we should deal with symlink here
                    // todo git modify ctime and mtime after status command
                    if meta.ctime_nsec() != ctime_ns || meta.mtime_nsec() != mtime_ns {
                        let data = std::fs::read(&abs_path)?;
                        let object = GitObject::new(Fmt::Blob, data);

                        let hash = gitlet::utils::sha(&object.serialize());
                        if hash != entry.sha {
                            println!("  modified: {}", entry.name);
                        }
                    }
                }
                all_files.remove(&repo.work_tree.join(&entry.name));
            }

            println!();

            println!("Untracked files:");

            for path in all_files {
                let path = path.strip_prefix(&repo.work_tree)?;
                if ignore.check(&path.to_string_lossy())?.unwrap_or(false) {
                    continue;
                }
                println!("  {}", path.display());
            }
        }
        Commands::Rm { path } => {
            let repo = Repository::find(".")?;

            repo.rm(path, true, false)?;
        }
    }
    Ok(())
}
