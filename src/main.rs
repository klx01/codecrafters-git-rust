use std::{fs, io};
use std::fs::File;
use std::io::{BufReader, BufWriter, stdout};
use anyhow::{bail, Context};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::io::prelude::*;
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use sha1::{Sha1, Digest};

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

#[derive(ValueEnum, Clone, Debug)]
enum ObjectType {
    Blob,
}
impl ObjectType {
    fn to_str(&self) -> &'static str {
        match self {
            ObjectType::Blob => "blob",
        }
    }
}

const OBJECTS_PATH: &'static str = ".git/objects";

fn main() -> anyhow::Result<()> {
    let a = Cli::parse();
    match a.command {
        Command::Init => init_command(),
        Command::CatFile {object, flags} => cat_file_command(object, flags),
        Command::HashObject { file, object_type, write } => hash_object_command(file, object_type, write),
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
        return print_object(&mut reader, size, &file_path);
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

fn read_object_type(reader: &mut impl BufRead, file_path: &str) -> anyhow::Result<String> {
    let mut buf = vec![];
    let delimiter = ' ' as u8;
    let read_size = reader.take(10).read_until(delimiter, &mut buf).context(format!("Failed to extract type from {file_path}"))?;
    if read_size == 0 {
        bail!("Failed to read object type from {file_path}, no data was read");
    }
    if buf[buf.len() - 1] != delimiter {
        bail!("Failed to read object type from {file_path}, delimiter not found");
    }

    let object_type = buf[..buf.len() - 1]
        .into_iter()
        .map(|x| *x as char)
        .collect::<String>();

    if (object_type != "blob") && (object_type != "tree") && (object_type != "commit") && (object_type != "tag") {
        bail!("Failed to extract type from {file_path}, invalid type {object_type}");
    }
    Ok(object_type)
}

fn read_object_size(reader: &mut impl BufRead, file_path: &str) -> anyhow::Result<u64> {
    let mut buf = vec![];
    let delimiter = 0;
    let read_size = reader.take(20).read_until(delimiter, &mut buf).context(format!("Failed to extract size from {file_path}"))?;
    if read_size == 0 {
        bail!("Failed to read object size from {file_path}, no data was read");
    }
    if buf[buf.len() - 1] != delimiter {
        bail!("Failed to read object size from {file_path}, delimiter not found");
    }

    let size_str = buf[..buf.len() - 1]
        .iter()
        .map(|x| *x as char)
        .collect::<String>();

    let size = size_str.parse::<u64>().context(format!("Failed to extract size from {file_path}: failed to convert to int {size_str}"))?;
    Ok(size)
}

fn print_object(reader: &mut impl BufRead, expected_size: u64, file_path: &str) -> anyhow::Result<()> {
    let mut writer = BufWriter::new(stdout().lock());
    let mut sized_reader = reader.take(expected_size);
    let copied_size = io::copy(&mut sized_reader, &mut writer).context(format!("Failed to output contents from {file_path}"))?;
    if copied_size != expected_size {
        bail!("unexpected content size: expected {expected_size} actual {copied_size}");
    }
    if let Ok(read_size) = reader.read(&mut [0]) {
        if read_size > 0 {
            bail!("content size is larger than expected {expected_size}");
        }
    }
    return Ok(());
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
