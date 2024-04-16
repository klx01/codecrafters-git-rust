use std::{fs, io};
use std::fs::File;
use std::io::{BufReader, BufWriter, stdout};
use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use std::io::prelude::*;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use sha1::{Sha1, Digest};
use git_starter_rust::object_read::{*};
use git_starter_rust::tree_object_read::TreeObjectIterator;

/// a subset of git, implemented as a learning challenge
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create an empty Git repository
    Init,

    /// Provide content for repository objects
    CatFile {
        #[clap(flatten)]
        flags: CatFlags,

        /// sha1 hash
        object: String,
    },

    /// Compute object ID and optionally create an object from a file
    HashObject {
        /// Specify the type of object to be created
        #[arg(value_enum, default_value = "blob", short = 't')]
        object_type: ObjectType,

        /// Actually write the object into the object database
        #[arg(short)]
        write: bool,

        /// file path
        file: String,
    },

    /// List the contents of a tree object
    LsTree {
        /// List only filenames
        #[arg(long)]
        name_only: bool,

        /// sha1 hash
        tree_sha: String,
    }
}

#[derive(Args)]
#[group(required = true, multiple = false)]
struct CatFlags {
    /// pretty-print <object> content
    #[arg(short = 'p')]
    print_content: bool,

    /// show object type
    #[arg(short = 't')]
    print_type: bool,

    /// show object size
    #[arg(short = 's')]
    print_size: bool,
}

fn main() -> anyhow::Result<()> {
    let a = Cli::parse();
    match a.command {
        Command::Init => init_command(),
        Command::CatFile {object, flags} => cat_file_command(object, flags),
        Command::HashObject { file, object_type, write } => hash_object_command(file, object_type, write),
        Command::LsTree { tree_sha, name_only } => ls_tree_command(tree_sha, name_only),
    }
}

fn init_command() -> anyhow::Result<()> {
    fs::create_dir(".git").context("Failed to create .git folder")?;
    fs::create_dir(OBJECTS_PATH).context(format!("Failed to create {OBJECTS_PATH} folder"))?;
    fs::create_dir(".git/refs").context("Failed to create .git/refs folder")?;
    fs::write(".git/HEAD", "ref: refs/heads/main\n").context("Failed to create .git/HEAD file")?;
    println!("Initialized git directory");
    Ok(())
}

fn cat_file_command(object: String, flags: CatFlags) -> anyhow::Result<()> {
    let object = find_and_decode_object(object)?;

    if flags.print_type {
        println!("{}", object.object_type.to_str());
        return Ok(());
    }
    if flags.print_size {
        println!("{}", object.size);
        return Ok(());
    }

    if flags.print_content {
        match object.object_type {
            ObjectType::Tree => {
                let iterator = TreeObjectIterator::from_decoded_object(object).unwrap();
                for item in iterator {
                    let item = item?;
                    println!(
                        "{:0>6} {} {}\t{}", 
                        item.attributes, 
                        item.get_type().to_str(),
                        item.hash,
                        item.file_path,
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
    let mut file = File::open(&file_name).context(format!("Failed to open file at {file_name}"))?;
    let meta = file.metadata().context(format!("Failed to extract metadata from {file_name}"))?;
    let header = format!("{} {}\0", object_type.to_str(), meta.len());

    let hash = calc_object_hash(&file, &header, &file_name)?;
    println!("{hash}");

    if write {
        file.rewind().context(format!("Failed to rewind file {file_name}"))?;
        write_object_file(&file, &header, &hash, &file_name)?;
    }

    Ok(())
}

fn calc_object_hash(file: &File, header: &str, file_name: &str) -> anyhow::Result<String> {
    let mut hasher = Sha1::new();
    hasher.update(header.as_bytes());
    drain_reader_into_hasher(&mut hasher, BufReader::new(file)).context(format!("Failed to calc file hash {file_name}"))?;
    let hash = hex::encode(hasher.finalize());
    Ok(hash)
}

fn drain_reader_into_hasher(hasher: &mut impl Digest, mut reader: impl BufRead) -> anyhow::Result<()> {
    loop {
        let buf = reader.fill_buf()?;
        let read_len = buf.len();
        if read_len == 0 {
            break;
        }
        hasher.update(buf);
        reader.consume(read_len);
    }
    Ok(())
}

fn write_object_file(orig_file: &File, header: &str, hash: &str, orig_file_name: &str) -> anyhow::Result<()> {
    let (dir, new_file_name) = hash.split_at(2);
    let new_file_path = format!("{OBJECTS_PATH}/{dir}/{new_file_name}");
    let new_file = File::create(&new_file_path).context(format!("Failed to create an object file {new_file_path}"))?;

    let mut encoder = ZlibEncoder::new(new_file, Compression::best());
    encoder.write(header.as_bytes()).context(format!("Failed to write compressed header data into {new_file_path}"))?;
    io::copy(&mut BufReader::new(orig_file), &mut encoder).context(format!("Failed to write compressed data from {orig_file_name} to {new_file_path}"))?;
    encoder.finish().context(format!("Failed to flush compressed data from to {new_file_path}"))?;

    Ok(())
}

fn ls_tree_command(object: String, name_only: bool) -> anyhow::Result<()> {
    if name_only {
        let object = find_and_decode_object(object)?;
        let iterator = TreeObjectIterator::from_decoded_object(object).unwrap();
        for item in iterator {
            let item = item?;
            println!("{}", item.file_path);
        }
        return Ok(());
    }

    cat_file_command(object, CatFlags {
        print_content: true,
        print_type: false,
        print_size: false,
    })
}
