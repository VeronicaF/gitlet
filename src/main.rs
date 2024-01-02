use anyhow::ensure;
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
    LsTree {
        /// Recurse into sub-trees
        #[arg(short)]
        recursive: bool,
        /// A tree-ish object.
        tree: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { path } => {
            let repo = gitlet::repository::Repository::init(path)?;
            println!("init at path: {}", repo.git_dir.display());
        }
        Commands::CatFile { fmt, object } => {
            let repo = gitlet::repo_find(".")?;

            let object = GitObject::read_object(&repo, &object)?;

            ensure!(object.header.fmt == fmt, "object type mismatch");

            println!("{}", object.display());
        }
        Commands::HashObject { write, fmt, path } => {
            let repo = gitlet::repo_find(".")?;
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
            let repo = gitlet::repo_find(".")?;
            print!(r"digraph wyaglog{{");
            print!("  node[shape=rect]");
            log_graphviz(&repo, commit, &mut BTreeSet::new())?;
            println!("}}");
        }
        Commands::LsTree { recursive, tree } => {
            let repo = gitlet::repo_find(".")?;

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
