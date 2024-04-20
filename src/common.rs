use std::error::Error;
use std::ffi::OsString;
use std::fmt::{Debug, Display, Formatter};
use std::fs;
use std::path::PathBuf;
use clap::ValueEnum;
use anyhow::Context;

pub const GIT_PATH: &'static str = ".git";
pub const OBJECTS_PATH: &'static str = ".git/objects";
pub const HEAD_PATH: &'static str = ".git/HEAD";

pub const TEST_REPO_PATH: &'static str = "test_data";

pub const MAX_OBJECT_SIZE: u64 = 1 * 1024 * 1024 * 1024; // 1 GB

pub const COMMIT_AUTHOR: &'static str =  "test";
pub const COMMIT_EMAIL: &'static str =  "example@example.com";
pub const COMMIT_TIMEZONE: &'static str =  "+0400";

pub const HASH_ENCODED_LEN: usize = 40;
pub const HASH_RAW_LEN: usize = 20;
pub const OBJECT_DIR_LEN: usize = 2;
pub const MIN_OBJECT_SEARCH_LEN: usize = 4;

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq)]
pub enum ObjectType {
    Blob,
    Tree,
    Commit,
    Tag,
}
impl ObjectType {
    pub fn to_str(&self) -> &'static str {
        match self {
            ObjectType::Blob => "blob",
            ObjectType::Tree => "tree",
            ObjectType::Commit => "commit",
            ObjectType::Tag => "blob",
        }
    }
}
impl AsRef<str> for ObjectType {
    fn as_ref(&self) -> &str {
        self.to_str()
    }
}
impl Display for ObjectType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.to_str(), f)
    }
}
impl TryFrom<&[u8]> for ObjectType {
    type Error = ConversionError;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value {
            b"blob" => Ok(Self::Blob),
            b"tree" => Ok(Self::Tree),
            b"commit" => Ok(Self::Commit),
            b"tag" => Ok(Self::Tag),
            _ => Err(ConversionError),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct TreeItem {
    pub mode: ObjectMode,
    pub file_name: OsString,
    pub hash: String,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ObjectMode {
    Normal = 100644,
    Executable = 100755,
    Symlink = 120000,
    Tree = 40000,
}
impl ObjectMode {
    pub fn get_type(&self) -> ObjectType {
        match self {
            Self::Tree => ObjectType::Tree,
            Self::Normal => ObjectType::Blob,
            Self::Executable => ObjectType::Blob,
            Self::Symlink => todo!("Handling symlinks is not implemented yet")
        }
    }
}
impl TryFrom<usize> for ObjectMode {
    type Error = ConversionError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            x if x == (Self::Normal as usize) => Ok(Self::Normal),
            x if x == (Self::Executable as usize) => Ok(Self::Executable),
            x if x == (Self::Symlink as usize) => Ok(Self::Symlink),
            x if x == (Self::Tree as usize) => Ok(Self::Tree),
            _ => Err(ConversionError),
        }
    }
}
impl Display for ObjectMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let int = *self as usize;
        Display::fmt(&int, f)
    }
}
#[derive(Debug)]
pub struct ConversionError;
impl Display for ConversionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConversionError")
    }
}
impl Error for ConversionError {}

pub fn get_object_path_by_hash(hash: &str) -> String {
    let (dir, new_file_name) = hash.split_at(OBJECT_DIR_LEN);
    format!("{OBJECTS_PATH}/{dir}/{new_file_name}")
}

pub fn get_hash_by_object_path(file_path: &str) -> String {
    let file_path = &file_path[OBJECTS_PATH.len() + 1..];
    let (dir, name) = file_path.split_once('/').unwrap();
    format!("{dir}{name}")
}

pub fn init_test() -> anyhow::Result<()> {
    /*
    alternatively i could provide the base dir as a param for all functions, but this seems much simpler
    this makes it glitch when running test in multiple threads
    i could fix this to work properly with multi threads, but it would still fail in other places
    for example, i'm using one temporary file with a constant name
     */
    let current_dir = std::env::current_dir().context("failed to get current dir")?;
    let test_dir = "test_data";
    if current_dir.file_name().unwrap().as_encoded_bytes() != test_dir.as_bytes() {
        std::env::set_current_dir(test_dir).context("failed to switch dir")?;
    }
    init_repo()?;
    Ok(())
}

pub fn init_repo() -> anyhow::Result<()> {
    fs::create_dir_all(GIT_PATH).context(format!("Failed to create {GIT_PATH} folder"))?;
    fs::create_dir_all(OBJECTS_PATH).context(format!("Failed to create {OBJECTS_PATH} folder"))?;
    fs::create_dir_all(".git/refs").context("Failed to create .git/refs folder")?;
    let head = PathBuf::from(".git/HEAD");
    if !head.exists() {
        fs::write(".git/HEAD", "ref: refs/heads/main\n").context("Failed to create .git/HEAD file")?;
    }
    Ok(())
}
