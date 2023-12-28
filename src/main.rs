use clap::{Parser, Subcommand};
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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { path } => {
            let repo = gitlet::repository::Repository::init(path)?;
            println!("init at path: {}", repo.git_dir.display());
        }
    }
    Ok(())
}
