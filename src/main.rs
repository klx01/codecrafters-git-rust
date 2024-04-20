use std::io::{BufWriter, stdout, Write};
use anyhow::{bail, Context};
use clap::{Parser};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::cli::{CatFlags, Cli, Command};
use crate::common::{COMMIT_AUTHOR, COMMIT_EMAIL, COMMIT_TIMEZONE, init_repo, ObjectType, TreeItem};
use crate::object_write::{hash_blob, hash_commit};
use crate::object_read::{*};
use crate::tree_object_read::TreeObjectIterator;
use crate::tree_object_write::hash_tree;

mod cli;
mod common;
mod object_read;
mod object_write;
mod tree_object_read;
mod tree_object_write;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => init_command(),
        Command::CatFile { object, flags, force_raw } => cat_file_command(object, flags, force_raw),
        Command::HashObject { file, object_type, write } => hash_object_command(file, object_type, write),
        Command::LsTree { tree_sha, name_only } => ls_tree_command(tree_sha, name_only),
        Command::WriteTree { dry_run } => write_tree_command(dry_run),
        Command::CommitTree { parent, message, dry_run, tree } => commit_tree_command(tree, parent, message, dry_run),
    }
}

fn init_command() -> anyhow::Result<()> {
    init_repo()?;
    println!("Initialized git directory");
    Ok(())
}

fn cat_file_command(object: String, flags: CatFlags, force_raw: bool) -> anyhow::Result<()> {
    let object = find_and_decode_object(&object)?;

    if flags.print_type {
        println!("{}", object.object_type);
        return Ok(());
    }
    if flags.print_size {
        println!("{}", object.size);
        return Ok(());
    }

    if flags.print_content {
        match (force_raw, object.object_type) {
            (false, ObjectType::Tree) => {
                let iterator = TreeObjectIterator::from_decoded_object(object).unwrap();
                for item in iterator {
                    let TreeItem {mode, file_name, hash} = item?;
                    let object_type = mode.get_type();
                    print!("{mode:0>6} {object_type} {hash}\t");
                    stdout().write(file_name.as_encoded_bytes())?;
                    print!("\n");
                }
            }
            _ => {
                let mut writer = BufWriter::new(stdout().lock());
                object.drain_into_writer_raw(&mut writer)?;
            },
        }
        return Ok(());
    }

    unreachable!("One of the flags should be always set");
}

fn hash_object_command(file_name: String, object_type: ObjectType, write: bool) -> anyhow::Result<()> {
    if object_type != ObjectType::Blob {
        bail!("Command is not implemented for {object_type}");
    }
    let path = Path::new(&file_name);
    let hash = hash_blob(path, write)?;
    println!("{hash}");

    Ok(())
}

fn ls_tree_command(object: String, name_only: bool) -> anyhow::Result<()> {
    if name_only {
        let object = find_and_decode_object(&object)?;
        let iterator = TreeObjectIterator::from_decoded_object(object).unwrap();
        for item in iterator {
            let item = item?;
            stdout().write(item.file_name.as_encoded_bytes())?;
        }
        return Ok(());
    }

    cat_file_command(
        object,
        CatFlags {
            print_content: true,
            print_type: false,
            print_size: false,
        },
        false
    )
}

fn write_tree_command(dry_run: bool) -> anyhow::Result<()> {
    let path = Path::new(".");
    let hash = hash_tree(path, !dry_run)?;
    let Some(hash) = hash else {
        bail!("Tree is empty");
    };
    println!("{hash}");
    Ok(())
}

fn commit_tree_command(tree: String, parent: Option<String>, message: String, dry_run: bool) -> anyhow::Result<()> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .context("Failed to get current timestamp")?
        .as_secs();

    let hash = hash_commit(
        &tree,
        parent.as_ref().map(|x| x.as_str()),
        &message,
        COMMIT_AUTHOR,
        COMMIT_EMAIL,
        timestamp,
        COMMIT_TIMEZONE,
        !dry_run,
    )?;
    println!("{hash}");

    Ok(())
}
