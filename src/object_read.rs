use std::{fs, io};
use std::fs::File;
use std::io::{Read, BufReader, Write};
use std::io::prelude::*;
use anyhow::{bail, Context};
use flate2::read::ZlibDecoder;
use crate::common::{get_hash_by_object_path, HASH_ENCODED_LEN, HASH_RAW_LEN, MAX_OBJECT_SIZE, MIN_OBJECT_SEARCH_LEN, OBJECT_DIR_LEN, OBJECTS_PATH, ObjectType};

pub(crate) struct LazyDecodedObject<R: Read> {
    pub file_path: String,
    pub object_type: ObjectType,
    pub size: u64,
    reader: R,
}
impl<R: Read> LazyDecodedObject<R> {
    pub fn drain_into_writer_raw(self, mut writer: &mut impl Write) -> anyhow::Result<(String, ObjectType, u64)> {
        let Self {file_path, object_type, size, mut reader} = self;
        let mut sized_reader = reader.by_ref().take(size);
        let copied_size = io::copy(&mut sized_reader, &mut writer).context(format!("Failed to copy contents from {file_path} to writer"))?;
        if copied_size != size {
            bail!("unexpected content size: expected {size} actual {copied_size}");
        }
        if !is_end_of_reader(reader) {
            bail!("content size is larger than expected {size}");
        }
        return Ok((file_path, object_type, size));
    }
    pub fn destruct(self) -> (String, ObjectType, u64, R) {
        let Self {file_path, object_type, size, reader} = self;
        (file_path, object_type, size, reader)
    }
    #[cfg(test)]
    pub fn destruct_into_string(self)  -> anyhow::Result<(String, ObjectType, u64, String)> {
        let mut vec = vec![];
        let (file_path, object_type, size) = self.drain_into_writer_raw(&mut vec)?;
        let str = vec.iter().map(|x| *x as char).collect();
        Ok((file_path, object_type, size, str))
    }
}

pub(crate) fn validate_existing_hash(hash: &str, expected_type: ObjectType) -> anyhow::Result<String> {
    let object = find_and_decode_object(&hash)?;
    if object.object_type != expected_type {
        bail!("Provided object {hash} is not a {}, it is actually a {}", expected_type.to_str(), object.object_type.to_str());
    }
    let hash = get_hash_by_object_path(&object.file_path);
    Ok(hash)
}

pub(crate) fn find_and_decode_object(object: &str) -> anyhow::Result<LazyDecodedObject<impl BufRead>> {
    let file_path = find_object_file(object)?;
    let mut reader = get_compressed_file_reader(&file_path)?;
    let object_type = read_object_type(&mut reader, &file_path)?;
    let size = read_object_size(&mut reader, &file_path)?;
    let res = LazyDecodedObject {
        file_path,
        object_type,
        size,
        reader,
    };
    Ok(res)
}

pub(crate) fn find_object_file(object: &str) -> anyhow::Result<String> {
    let len = object.len();
    if (len < MIN_OBJECT_SEARCH_LEN) || (len > HASH_ENCODED_LEN) {
        bail!("Invalid object name {object}");
    }
    let (dir, file_search) = object.split_at(OBJECT_DIR_LEN);
    let dir_path = format!("{OBJECTS_PATH}/{dir}/");

    let mut found_name = None;
    let dir_files = fs::read_dir(&dir_path).context(format!("Failed to read dir {dir_path}"))?;
    for dir_entry in dir_files {
        let dir_entry = dir_entry.context(format!("Some weird error while reading file name in {dir_path}"))?;
        let file_name_os = dir_entry.file_name();
        let Some(file_name) = file_name_os.to_str() else {
            bail!("Failed to convert file name to str {file_name_os:?}");
        };
        if file_name.len() != (HASH_ENCODED_LEN - OBJECT_DIR_LEN) {
            continue;
        }
        if !file_name.starts_with(file_search) {
            continue;
        }
        if found_name.is_none() {
            found_name = Some(file_name.to_string());
        } else {
            bail!("Found multiple objects starting with {object}");
        }
    }
    let Some(found_name) = found_name else {
        bail!("Found no objects starting with {object}");
    };

    let file_path = format!("{dir_path}{found_name}");
    Ok(file_path)
}

fn get_compressed_file_reader(file_path: &str) -> anyhow::Result<impl BufRead> {
    let file = File::open(file_path).context(format!("Failed to open object file at {file_path}"))?;
    let decoder = ZlibDecoder::new(file);
    let reader = BufReader::new(decoder);
    Ok(reader)
}

fn read_object_type(reader: &mut impl BufRead, file_path: &str) -> anyhow::Result<ObjectType> {
    let mut buf = vec![];
    let delimiter = b' ';
    let read_size = reader.take(10).read_until(delimiter, &mut buf).context(format!("Failed to extract type from {file_path}"))?;
    if read_size == 0 {
        bail!("Failed to read object type from {file_path}, no data was read");
    }
    let (last, data) = buf.split_last().unwrap();
    if *last != delimiter {
        bail!("Failed to read object type from {file_path}, delimiter not found");
    }

    let object_type = ObjectType::try_from(data).context(format!("Failed to extract type from {file_path}, invalid type"))?;

    Ok(object_type)
}

fn read_object_size(reader: &mut impl BufRead, file_path: &str) -> anyhow::Result<u64> {
    let mut buf = vec![];
    let delimiter = 0;
    let read_size = reader.take(HASH_RAW_LEN as u64).read_until(delimiter, &mut buf).context(format!("Failed to extract size from {file_path}"))?;
    if read_size == 0 {
        bail!("Failed to read object size from {file_path}, no data was read");
    }
    let (last, data) = buf.split_last().unwrap();
    if *last != delimiter {
        bail!("Failed to read object size from {file_path}, delimiter not found");
    }

    let size_str = data
        .iter()
        .map(|x| *x as char)
        .collect::<String>();

    let size = size_str.parse::<u64>().context(format!("Failed to extract size from {file_path}: failed to convert to int {size_str}"))?;
    if size > MAX_OBJECT_SIZE {
        bail!("Object size {size} is larger than max allowed size {MAX_OBJECT_SIZE} in {file_path}");
    }

    Ok(size)
}

pub(crate) fn is_end_of_reader(mut reader: impl Read) -> bool {
    let result = reader.read(&mut [0]);
    match result {
        Ok(0) => true,
        Ok(_) => false,
        Err(_) => true, // todo: should this be true?
    }
}
