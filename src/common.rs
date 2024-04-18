use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use clap::ValueEnum;

pub const GIT_PATH: &'static str = ".git";
pub const OBJECTS_PATH: &'static str = ".git/objects";
pub const MAX_OBJECT_SIZE: u64 = 1 * 1024 * 1024 * 1024; // 1 GB
pub const COMMIT_AUTHOR: &'static str =  "test";
pub const COMMIT_EMAIL: &'static str =  "example@example.com";
pub const COMMIT_TIMEZONE: &'static str =  "+0400";

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq)]
pub enum ObjectType {
    Blob,
    Tree,
    Commit,
    Tag,
}
impl ObjectType {
    pub fn from_raw_str(str: &[u8]) -> Option<Self> {
        match str {
            b"blob" => Some(Self::Blob),
            b"tree" => Some(Self::Tree),
            b"commit" => Some(Self::Commit),
            b"tag" => Some(Self::Tag),
            _ => None,
        }
    }
    pub fn to_str(&self) -> &'static str {
        match self {
            ObjectType::Blob => "blob",
            ObjectType::Tree => "tree",
            ObjectType::Commit => "commit",
            ObjectType::Tag => "blob",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct TreeItem {
    pub mode: ObjectMode,
    pub file_name: String,
    pub hash: String,
}
impl TreeItem {
    pub fn get_type(&self) -> ObjectType {
        match self.mode {
            ObjectMode::Tree => ObjectType::Tree,
            ObjectMode::Normal => ObjectType::Blob,
            ObjectMode::Executable => ObjectType::Blob,
            ObjectMode::Symlink => todo!("Handling symlinks is not implemented yet")
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ObjectMode {
    Normal = 100644,
    Executable = 100755,
    Symlink = 120000,
    Tree = 40000,
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
#[derive(Debug)]
pub struct ConversionError;
impl Display for ConversionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConversionError")
    }
}
impl Error for ConversionError {}

pub fn make_object_header(object_type: ObjectType, size: u64) -> String {
    format!("{} {}\0", object_type.to_str(), size)
}

pub fn get_object_path_by_hash(hash: &str) -> String {
    let (dir, new_file_name) = hash.split_at(2);
    format!("{OBJECTS_PATH}/{dir}/{new_file_name}")
}

pub fn get_hash_by_object_path(file_path: &str) -> String {
    let file_path = &file_path[OBJECTS_PATH.len() + 1..];
    let (dir, name) = file_path.split_once('/').unwrap();
    format!("{dir}{name}")
}
