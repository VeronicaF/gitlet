use anyhow::ensure;
use clap::{Parser, Subcommand};
use gitlet::object::{Fmt, GitObject};
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
    }
    Ok(())
}
