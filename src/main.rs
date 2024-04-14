use std::{fs, io};
use std::fs::File;
use std::io::{BufReader, BufWriter, stdout};
use anyhow::{bail, Context};
use clap::{Args, Parser, Subcommand};
use std::io::prelude::*;
use flate2::read::ZlibDecoder;

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
        /// sha1 hash
        object: String,

        #[clap(flatten)]
        flags: CatFlags,
    },
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

const OBJECTS_PATH: &'static str = ".git/objects";

fn main() -> anyhow::Result<()> {
    let a = Cli::parse();
    match a.command {
        Command::Init => init_command(),
        Command::CatFile {object, flags} => cat_file_command(object, flags),
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
    let file_path = find_object_file(object)?;
    let mut reader = get_compressed_file_reader(&file_path)?;

    let object_type = read_object_type(&mut reader, &file_path)?;
    if flags.print_type {
        println!("{object_type}");
        return Ok(());
    }

    let size = read_object_size(&mut reader, &file_path)?;
    if flags.print_size {
        println!("{size}");
        return Ok(());
    }

    if flags.print_content {
        let mut writer = BufWriter::new(stdout().lock());
        io::copy(&mut reader, &mut writer).context(format!("Failed to output contents from {file_path}"))?;
        return Ok(());
    }

    unreachable!("One of the flags should be always set");
}

fn find_object_file(object: String) -> anyhow::Result<String> {
    let len = object.len();
    if (len < 4) || (len > 40) {
        bail!("Invalid object name {object}");
    }
    let (dir, file_search) = object.split_at(2);
    let dir_path = format!("{OBJECTS_PATH}/{dir}/");

    let mut found_name = None;
    let dir_files = fs::read_dir(&dir_path).context(format!("Failed to read dir {dir_path}"))?;
    for dir_entry in dir_files {
        let dir_entry = dir_entry.context(format!("Some weird error while reading file name in {dir_path}"))?;
        let file_name_os = dir_entry.file_name();
        let Some(file_name) = file_name_os.to_str() else {
            bail!("Failed to convert file name to str {file_name_os:?}");
        };
        if file_name.starts_with(file_search) {
            if found_name.is_none() {
                found_name = Some(file_name.to_string());
            } else {
                bail!("Found multiple objects starting with {object}");
            }
        }
    }
    let Some(found_name) = found_name else {
        bail!("Found multiple objects starting with {object}");
    };

    let file_path = format!("{dir_path}/{found_name}");
    Ok(file_path)
}

fn get_compressed_file_reader(file_path: &String) -> anyhow::Result<impl BufRead> {
    let file = File::open(&file_path).context(format!("Failed to open file at {file_path}"))?;
    let decoder = ZlibDecoder::new(file);
    let reader = BufReader::new(decoder);
    Ok(reader)
}

fn read_object_type(reader: &mut impl BufRead, file_path: &String) -> anyhow::Result<String> {
    let mut buf = vec![];
    let _ = reader.take(10).read_until(' ' as u8, &mut buf).context(format!("Failed to extract type from {file_path}"))?;

    let object_type = buf[..buf.len() - 1]
        .into_iter()
        .map(|x| *x as char)
        .collect::<String>();

    if (object_type != "blob") && (object_type != "tree") && (object_type != "commit") && (object_type != "tag") {
        bail!("Failed to extract type from {file_path}, invalid type {object_type}");
    }
    Ok(object_type)
}

fn read_object_size(reader: &mut impl BufRead, file_path: &String) -> anyhow::Result<u64> {
    let mut buf = vec![];
    let _ = reader.take(20).read_until(0, &mut buf).context(format!("Failed to extract size from {file_path}"))?;

    let size_str = buf[..buf.len() - 1]
        .iter()
        .map(|x| *x as char)
        .collect::<String>();

    let size = size_str.parse::<u64>().context(format!("Failed to extract size from {file_path}: failed to convert to int {size_str}"))?;
    Ok(size)
}
