use std::fs;
use std::io::{BufWriter, stdout};
use anyhow::{bail, Context};
use clap::{Parser};
use std::path::PathBuf;
use git_starter_rust::cli::{CatFlags, Cli, Command};
use git_starter_rust::common::{GIT_PATH, OBJECTS_PATH, ObjectType};
use git_starter_rust::object_write::{create_blob_object, write_object_file};
use git_starter_rust::object_read::{*};
use git_starter_rust::tree_object_read::TreeObjectIterator;
use git_starter_rust::tree_object_write::{create_and_write_tree_object, create_tree_object};


fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => init_command(),
        Command::CatFile { object, flags, force_raw } => cat_file_command(object, flags, force_raw),
        Command::HashObject { file, object_type, write } => hash_object_command(file, object_type, write),
        Command::LsTree { tree_sha, name_only } => ls_tree_command(tree_sha, name_only),
        Command::WriteTree { dry_run } => write_tree_command(dry_run),
    }
}

fn init_command() -> anyhow::Result<()> {
    fs::create_dir(GIT_PATH).context(format!("Failed to create {GIT_PATH} folder"))?;
    fs::create_dir(OBJECTS_PATH).context(format!("Failed to create {OBJECTS_PATH} folder"))?;
    fs::create_dir(".git/refs").context("Failed to create .git/refs folder")?;
    fs::write(".git/HEAD", "ref: refs/heads/main\n").context("Failed to create .git/HEAD file")?;
    println!("Initialized git directory");
    Ok(())
}

fn cat_file_command(object: String, flags: CatFlags, force_raw: bool) -> anyhow::Result<()> {
    let object = find_and_decode_object(&object)?;

    if flags.print_type {
        println!("{}", object.object_type.to_str());
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
                    let item = item?;
                    println!(
                        "{:0>6} {} {}\t{}",
                        item.mode as usize,
                        item.get_type().to_str(),
                        item.hash,
                        item.file_name,
                    );
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
        bail!("Command is not implemented for {}", object_type.to_str());
    }
    let path = PathBuf::from(file_name);
    let object = create_blob_object(path, write)?;
    println!("{}", object.hash);

    if write {
        write_object_file(object)?;
    }

    Ok(())
}

fn ls_tree_command(object: String, name_only: bool) -> anyhow::Result<()> {
    if name_only {
        let object = find_and_decode_object(&object)?;
        let iterator = TreeObjectIterator::from_decoded_object(object).unwrap();
        for item in iterator {
            let item = item?;
            println!("{}", item.file_name);
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
    let path = PathBuf::from(".");
    let hash = if dry_run {
        let tree = create_tree_object(path)?;
        tree.hash
    } else {
        create_and_write_tree_object(path)?
    };
    println!("{hash}");
    Ok(())
}
