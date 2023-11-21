use clap::{Parser, Subcommand};

use drake::Drake;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a path and index packages
    Packages {
        /// Path to scan
        #[arg(default_value = ".")]
        path: String,
    },
    /// Scan a path and index declarations and references
    Types {
        /// Path to scan
        #[arg(default_value = ".")]
        path: String,
    },
    /// Print contents of specific files
    Print {
        /// Path to scan
        #[arg(default_value = ".")]
        path: String,
        #[arg(short, long = "decl")]
        declarations: bool,
        #[arg(short, long = "refs")]
        references: bool,
        #[arg(long)]
        full: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut drake = Drake::new();

    match &cli.command {
        Command::Packages { path } => drake.package_name(path)?,
        Command::Types { path } => drake.types(path)?,
        Command::Print {
            path,
            declarations,
            references,
            full,
        } => drake.print(path, *declarations, *references, *full)?,
    }

    Ok(())
}
