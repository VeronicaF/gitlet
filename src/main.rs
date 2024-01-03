use anyhow::ensure;
use bytes::Bytes;
use clap::{Parser, Subcommand};
use gitlet::object::{Fmt, GitObject};
use gitlet::repository::Repository;
use std::collections::BTreeSet;
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
        #[arg(help = "The object to display.")]
        object: String,
    },

    /// Compute object ID and optionally creates a blob from a file
    HashObject {
        /// Actually write the object into the database
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
        /// Read object from <file>
        path: PathBuf,
    },

    /// Display history of a given commit.
    Log {
        /// Commit to start at
        #[arg(default_value = "HEAD")]
        commit: String,
    },
    /// List the contents of a tree object
    LsTree {
        /// Recurse into sub-trees
        #[arg(short)]
        recursive: bool,
        /// A tree-ish object.
        tree: String,
    },

    /// Checkout a commit inside of a directory.
    /// todo this just clones file by tree into the directory, does not update HEAD
    Checkout {
        /// The commit or tree to checkout.
        commit: String,
        /// The EMPTY directory to checkout on.
        path: PathBuf,
    },
    /// List all refs in a local repository
    ShowRef,
    /// tag
    Tag {
        /// Whether to create a tag object
        #[arg(short = 'a', requires = "name")]
        create_tag_object: bool,
        /// The new tag's name.
        name: Option<String>,
        /// The object the new tag will point to
        #[arg(default_value = "HEAD")]
        object: String,
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

            let object = GitObject::read_object(&repo, &object)?;

            ensure!(object.header.fmt == fmt, "object type mismatch");

            println!("{}", object.display());
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
            print!(r"digraph wyaglog{{");
            print!("  node[shape=rect]");
            log_graphviz(&repo, commit, &mut BTreeSet::new())?;
            println!("}}");
        }
        Commands::LsTree { recursive, tree } => {
            let repo = Repository::find(".")?;

            fn ls_tree(
                repo: &Repository,
                recursive: bool,
                tree: String,
                prefix: PathBuf,
            ) -> anyhow::Result<()> {
                let tree_object = GitObject::read_object(repo, &tree)?;

                ensure!(tree_object.header.fmt == Fmt::Tree, "object type mismatch");

                let tree = gitlet::object::tree::Tree::parse(tree_object.data)?;

                for (mode, path, sha1) in tree.0 {
                    let file_type = sha1.file_type.to_str();
                    let sha1_str = sha1.sha1;
                    let mode = mode.0;
                    if recursive && sha1.file_type == gitlet::object::tree::FileType::Tree {
                        ls_tree(repo, recursive, sha1_str, prefix.join(path))?;
                    } else {
                        println!(
                            "{} {} {}\t{}",
                            mode,
                            file_type,
                            sha1_str,
                            prefix.join(&path).display()
                        );
                    }
                }

                Ok(())
            }

            ls_tree(&repo, recursive, tree, PathBuf::from(""))?;
        }
        Commands::Checkout { commit, path } => {
            let repo = Repository::find(".")?;

            let commit = GitObject::read_object(&repo, &commit)?;
            ensure!(
                commit.header.fmt == Fmt::Commit,
                "object type mismatch, expected commit"
            );

            let commit = gitlet::object::commit::Commit::new(&commit.data);

            let tree = commit.tree().ok_or(anyhow::anyhow!("commit has no tree"))?;
            let tree = String::from_utf8_lossy(&tree).to_string();

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

            fn checkout(repo: &Repository, tree: String, prefix: PathBuf) -> anyhow::Result<()> {
                let tree_object = GitObject::read_object(repo, &tree)?;
                ensure!(
                    tree_object.header.fmt == Fmt::Tree,
                    "object type mismatch, expected tree"
                );
                let tree = gitlet::object::tree::Tree::parse(tree_object.data)?;

                for (.., path, sha1) in tree.0 {
                    let object = GitObject::read_object(repo, &sha1.sha1)?;
                    let dest = prefix.join(&path);

                    match sha1.file_type {
                        gitlet::object::tree::FileType::Tree => {
                            std::fs::create_dir_all(&dest)?;
                            checkout(repo, sha1.sha1, dest)?;
                        }
                        gitlet::object::tree::FileType::Blob => {
                            std::fs::write(&dest, object.data)?;
                        }
                        gitlet::object::tree::FileType::SymLink => {
                            unimplemented!()
                        }
                        gitlet::object::tree::FileType::Commit => {
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
            let tags_path = repo.git_dir.join("refs").join("tags");

            if let Some(name) = name {
                let sha = repo.find_object(object);

                // create tag
                if create_tag_object {
                    // todo refactor the git object and specific stuff
                    // todo this is stupid and ugly
                    let mut kvlm = gitlet::object::kvlm::Kvlm::default();
                    kvlm.insert(Bytes::from("object"), vec![Bytes::from(sha)]);
                    kvlm.insert(Bytes::from("type"), vec![Bytes::from("commit")]);
                    kvlm.insert(Bytes::from("tag"), vec![Bytes::from(name.clone())]);
                    kvlm.insert(
                        Bytes::from("tagger"),
                        vec![Bytes::from("default@default.com")],
                    );
                    kvlm.insert(
                        Bytes::from("message"),
                        vec![Bytes::from(
                            "A tag generated by gitlet, which won't let you customize the message!",
                        )],
                    );

                    let bytes = kvlm.serialize();

                    let git_object = GitObject::new(Fmt::Tag, bytes.into());

                    let sha = git_object.write_object(&repo)?;

                    std::fs::write(tags_path.join(name), sha)?;
                } else {
                    std::fs::write(tags_path.join(name), sha)?;
                }
            } else {
                // list tags
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
    }
    Ok(())
}

// todo do not clone
fn log_graphviz(
    repo: &Repository,
    sha: String,
    visited: &mut BTreeSet<String>,
) -> anyhow::Result<()> {
    if visited.contains(&sha) {
        return Ok(());
    }

    visited.insert(sha.clone());

    let commit = GitObject::read_object(repo, &sha)?;

    anyhow::ensure!(commit.header.fmt == Fmt::Commit, "object type mismatch");

    let commit = gitlet::object::commit::Commit::new(&commit.data);
    let short_sha = &sha[..8];

    let mut message = commit
        .message()
        .unwrap_or_default()
        .replace('\\', "\\\\")
        .replace('\"', "\\\"");

    if let Some(i) = message.find('\n') {
        message = message[..i].to_owned();
    }

    print!("  c_{} [label=\"{}: {}\"]", sha, short_sha, message);

    if let Some(parents) = commit.parents() {
        for parent in parents {
            let parent = String::from_utf8_lossy(&parent).to_string();
            print!("  c_{} -> c_{}", sha, parent);
            log_graphviz(repo, parent, visited)?;
        }
    }

    Ok(())
}
