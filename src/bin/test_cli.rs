use clap::{ArgGroup, Args, Parser, Subcommand};

/// 这是我们的主命令行程序
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

/// 定义子命令和相关参数
#[derive(Subcommand)]
enum Commands {
    /// tag 子命令
    Tag(TagArgs),
}

/// tag 命令的参数
#[derive(Args)]
#[clap(group(ArgGroup::new("group").args(&["a", "b", "c"])))] // 设置参数分组
struct TagArgs {
    /// -a 参数
    #[clap(short, long, group = "group")]
    a: bool,

    /// -b 参数
    #[clap(short, long, group = "group")]
    b: bool,

    /// -c 参数
    #[clap(short, long, group = "group")]
    c: bool,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Tag(args) => {
            if args.a {
                println!("Option -a is used.");
            }
            if args.b {
                println!("Option -b is used.");
            }
            if args.c {
                println!("Option -c is used.");
            }
            if !args.a && !args.b && !args.c {
                println!("No options provided for 'tag'.");
            }
        }
    }
}
