use std::fs::File;
use std::{fs, io};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use sha1::{Digest, Sha1};
use crate::common::{get_object_path_by_hash, ObjectType};
use anyhow::{bail, Context};
use std::io::prelude::*;
use std::path::Path;
use crate::object_read::validate_existing_hash;

struct HashWriter<W: Write, H: Digest> {
    hasher: H,
    writer: W,
}
impl<W: Write, H: Digest> Write for HashWriter<W, H> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written_size = self.writer.write(buf)?;
        self.hasher.update(&buf[..written_size]);
        Ok(written_size)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()?;
        Ok(())
    }
}

static TEMPORARY_FILE: &'static str = ".git/temp_file";

pub(crate) fn hash_blob(path: &Path, write_file: bool) -> anyhow::Result<String> {
    let file = File::open(path).context(format!("Failed to open file at {}", path.display()))?;
    let meta = file.metadata().context(format!("Failed to extract metadata from {}", path.display()))?;
    hash_object(file, ObjectType::Blob, meta.len(), write_file)
}

pub(crate) fn hash_commit(tree: &str, parent: Option<&str>, message: &str, author: &str, email: &str, timestamp: u64, timezone: &str, write_file: bool) -> anyhow::Result<String> {
    let data = create_commit_body(tree, parent, message, author, email, timestamp, timezone)?;
    let hash = hash_object(data.as_bytes(), ObjectType::Commit, data.as_bytes().len() as u64, write_file)?;
    Ok(hash)
}

fn create_commit_body(tree: &str, parent: Option<&str>, message: &str, author: &str, email: &str, timestamp: u64, timezone: &str) -> anyhow::Result<String> {
    let tree = validate_existing_hash(tree, ObjectType::Tree)?;

    let parent_line = match parent {
        Some(parent) => {
            let parent = validate_existing_hash(parent, ObjectType::Commit)?;
            format!("\nparent {parent}")
        }
        None => String::new(),
    };

    let data = format!("tree {tree}{parent_line}
author {author} <{email}> {timestamp} {timezone}
committer {author} <{email}> {timestamp} {timezone}

{message}
");
    Ok(data)
}

pub(crate) fn hash_object(reader: impl Read, object_type: ObjectType, size: u64, write_file: bool) -> anyhow::Result<String> {
    let hash = if write_file {
        let writer = get_temporary_file_writer()?;
        let hash = hash_write(reader, object_type, size, writer)?;
        move_temporary_file(&hash)?;
        hash
    } else {
        hash_write(reader, object_type, size, io::sink())?
    };
    Ok(hash)
}

fn hash_write(mut reader: impl Read, object_type: ObjectType, size: u64, writer: impl Write) -> anyhow::Result<String> {
    let hasher = Sha1::new();
    let mut writer = HashWriter {hasher, writer};
    let header = format!("{object_type} {size}\0");
    writer.write(header.as_bytes()).context("Failed to hash and write header")?;
    let copied_size = io::copy(&mut reader, &mut writer).context("Failed to hash and write data")?;
    writer.flush().context("Failed to flush data")?;
    if copied_size != size {
        bail!("invalid data size: expected {size}, actual {copied_size}");
    }
    let hash = writer.hasher.finalize();
    let hash = hex::encode(hash);
    Ok(hash)
}

fn get_temporary_file_writer() -> anyhow::Result<impl Write> {
    let file = File::create(TEMPORARY_FILE).context(format!("Failed to create the temp file at {TEMPORARY_FILE}"))?;
    let encoder = ZlibEncoder::new(file, Compression::best());
    Ok(encoder)
}

fn move_temporary_file(hash: &str) -> anyhow::Result<()> {
    let path_str = get_object_path_by_hash(hash);
    let path = Path::new(&path_str);
    let dir_path = path.parent().unwrap();
    if !dir_path.exists() {
        fs::create_dir(dir_path).context(format!("Failed to create folder at {}", dir_path.display()))?;
    }
    fs::rename(TEMPORARY_FILE, path).context(format!("Failed move temporary file to {path_str}"))?;
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::common::{COMMIT_AUTHOR, COMMIT_EMAIL, COMMIT_TIMEZONE, init_test};
    use crate::object_read::find_and_decode_object;
    use crate::tree_object_write::hash_tree;
    use super::*;

    #[test]
    fn test_hash_blob() -> anyhow::Result<()> {
        init_test()?;
        let path = Path::new("data/data.txt");
        let hash = hash_blob(path, true)?;
        assert_eq!("bae42c55f9e0a4e297a4d197d8aadfe147ef269b", hash);

        let expected_data = "test1\ntest2\n";
        let (file_path, object_type, size, actual_data) = find_and_decode_object(&hash)?.destruct_into_string()?;
        assert_eq!(ObjectType::Blob, object_type);
        assert_eq!(expected_data.len(), size as usize);
        assert_eq!(".git/objects/ba/e42c55f9e0a4e297a4d197d8aadfe147ef269b", file_path);
        assert_eq!(expected_data, actual_data);

        Ok(())
    }

    #[test]
    fn test_hash_commit() -> anyhow::Result<()> {
        init_test()?;
        let path = Path::new(".");
        let tree = hash_tree(path, true)?.unwrap();
        assert_eq!("0b70d742c267c707ebd81d8968fc2e696a9e2edb", tree);

        let author = COMMIT_AUTHOR;
        let email = COMMIT_EMAIL;
        let message = "test message";
        let parent = None;
        let timestamp = 1713381411;
        let timezone = COMMIT_TIMEZONE;

        let hash = hash_commit(&tree, parent, message, author, email, timestamp, timezone, true)?;
        assert_eq!("810e2b66b9a81b642795d05af640fa4a2f5fe269", hash);
        let (file_path, object_type, size, actual_data) = find_and_decode_object(&hash)?.destruct_into_string()?;
        let expected_data =
"tree 0b70d742c267c707ebd81d8968fc2e696a9e2edb
author test <example@example.com> 1713381411 +0400
committer test <example@example.com> 1713381411 +0400

test message
";
        assert_eq!(ObjectType::Commit, object_type);
        assert_eq!(expected_data.len(), size as usize);
        assert_eq!(".git/objects/81/0e2b66b9a81b642795d05af640fa4a2f5fe269", file_path);
        assert_eq!(expected_data, actual_data);

        let parent = hash.as_str();
        let hash = hash_commit(&tree, Some(parent), message, author, email, timestamp, timezone, true)?;
        assert_eq!("eed950c7ed93db7ab0e15de6821498e5c9a826f5", hash);
        let (file_path, object_type, size, actual_data) = find_and_decode_object(&hash)?.destruct_into_string()?;
        let expected_data =
"tree 0b70d742c267c707ebd81d8968fc2e696a9e2edb
parent 810e2b66b9a81b642795d05af640fa4a2f5fe269
author test <example@example.com> 1713381411 +0400
committer test <example@example.com> 1713381411 +0400

test message
";
        assert_eq!(ObjectType::Commit, object_type);
        assert_eq!(expected_data.len(), size as usize);
        assert_eq!(".git/objects/ee/d950c7ed93db7ab0e15de6821498e5c9a826f5", file_path);
        assert_eq!(expected_data, actual_data);

        let same = hash_commit(&tree[..20], Some(&parent[..20]), message, author, email, timestamp, timezone, true)?;
        assert_eq!(hash, same);

        let res = hash_commit(&tree, Some(&tree), message, author, email, timestamp, timezone, true);
        assert!(res.is_err());

        let res = hash_commit(parent, Some(parent), message, author, email, timestamp, timezone, true);
        assert!(res.is_err());

        Ok(())
    }
}
