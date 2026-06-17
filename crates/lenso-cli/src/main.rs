mod host;

use clap::{Parser, Subcommand};

/// Lenso command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "lenso",
    version,
    about = "Scaffold and operate Lenso backend projects",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scaffold and manage Lenso host applications.
    Host {
        #[command(subcommand)]
        command: HostCommand,
    },
}

#[derive(Debug, Subcommand)]
enum HostCommand {
    /// Create a new Lenso host application in a target directory.
    Init {
        /// Target directory for the new project.
        dir: String,

        /// Package name for the generated Cargo crate.
        ///
        /// Defaults to the target directory name. Must be a valid Cargo crate name.
        #[arg(long)]
        name: Option<String>,

        /// Allow scaffolding into a non-empty directory.
        #[arg(long)]
        force: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Host { command } => match command {
            HostCommand::Init { dir, name, force } => host::init(&dir, name.as_deref(), force)?,
        },
    }

    Ok(())
}
