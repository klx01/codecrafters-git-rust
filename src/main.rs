use std::fs;
use anyhow::Context;
use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
}

fn main() -> anyhow::Result<()> {
    let a = Cli::parse();
    match a.command {
        Command::Init => {
            fs::create_dir(".git").context("Failed to create .git folder")?;
            fs::create_dir(".git/objects").context("Failed to create .git/objects folder")?;
            fs::create_dir(".git/refs").context("Failed to create .git/refs folder")?;
            fs::write(".git/HEAD", "ref: refs/heads/main\n").context("Failed to create .git/HEAD file")?;
            println!("Initialized git directory");
        }
    }
    Ok(())
}
