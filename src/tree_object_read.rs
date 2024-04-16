use std::io::{BufRead, Read};
use anyhow::{bail, Context};
use crate::object_read::{LazyDecodedObject, ObjectType};

pub struct TreeObjectIterator<R: BufRead> {
    pub file_path: String,
    pub size: u64,
    reader: Option<R>,
    entry_no: usize,
    bytes_read: u64,
}

pub struct TreeEntry {
    pub attributes: usize,
    pub file_path: String,
    pub hash: String,
}

impl TreeEntry {
    pub fn get_type(&self) -> ObjectType {
        if self.attributes == 40000 {
            ObjectType::Tree
        } else {
            ObjectType::Blob
        }
    }
}

impl<R: BufRead> TreeObjectIterator<R> {
    pub fn from_decoded_object(object: LazyDecodedObject<R>) -> Option<Self> {
        let (file_path, object_type, size, reader) = object.destruct();
        if object_type == ObjectType::Tree {
            let res = Self {
                file_path,
                size,
                reader: Some(reader),
                entry_no: 0,
                bytes_read: 0,
            };
            Some(res)
        } else {
            None
        }
    }
    fn parse(&mut self) -> anyhow::Result<Option<TreeEntry>> {
        let res = self.parse_inner();
        match res {
            Ok(x) => Ok(x),
            Err(x) => {
                self.reader.take();
                Err(x)
            }
        }
    }
    fn parse_inner(&mut self) -> anyhow::Result<Option<TreeEntry>> {
        /*
        this method is a bit weird because i need to have both the original reader and a view into it with a limited size
        it would be a bit more simple if i could put both of them in one struct
         */
        let reader = self.reader.as_mut();
        let Some(reader) = reader else {
            return Ok(None);
        };

        assert!(self.size >= self.bytes_read); // it should not be possible to read more than size
        let size_left = self.size - self.bytes_read;
        let mut sized_reader = reader.by_ref().take(size_left);
        self.entry_no += 1;

        let attrs = Self::parse_attributes(&mut sized_reader, self.entry_no, &self.file_path)?;
        let Some(attrs) = attrs else {
            if crate::object_read::is_end_of_reader(self.reader.take().unwrap()) {
                return Ok(None);
            } else {
                bail!("content size is larger than expected {}", self.size);
            };
        };
        let attrs_len = attrs.as_bytes().len();
        let attrs = attrs.parse().context(format!("Failed to pars attributes as int for entry {} from {}", self.entry_no, self.file_path))?;

        let name = Self::parse_name(&mut sized_reader, self.entry_no, &self.file_path)?;
        let sha = Self::parse_sha(&mut sized_reader, self.entry_no, &self.file_path)?;

        let bytes_read =
            attrs_len
                + 1 // delimiter ' '
                + name.as_bytes().len()
                + 1  // delimiter '\0'
                + 20; // sha
        self.bytes_read += bytes_read as u64;

        let res = TreeEntry {
            attributes: attrs,
            file_path: name,
            hash: sha,
        };
        Ok(Some(res))
    }
    fn parse_attributes(reader: &mut impl BufRead, entry: usize, file_path: &String) -> anyhow::Result<Option<String>> {
        let mut buffer = vec![];
        let delimiter = ' ' as u8;
        reader.take(10).read_until(delimiter, &mut buffer)
            .context(format!("Failed to read attributes for entry {entry} from {file_path}"))?;
        let Some((last, attrs)) = buffer.split_last() else {
            return Ok(None);
        };
        if *last != delimiter {
            bail!("Failed to read attributes for entry {entry} from {file_path}, delimiter not found");
        }
        if attrs.len() == 0 {
            bail!("Failed to read attributes for entry {entry} from {file_path}: empty name");
        }
        let attrs = attrs.into_iter().map(|x| *x as char).collect();
        Ok(Some(attrs))
    }
    fn parse_name(reader: &mut impl BufRead, entry: usize, file_path: &String) -> anyhow::Result<String> {
        let mut buffer = vec![];
        let name_delimiter = 0;
        reader.read_until(name_delimiter, &mut buffer)
            .context(format!("Failed to read file name for entry {entry} from {file_path}"))?;
        let Some((last, name)) = buffer.split_last() else {
            bail!("Failed to read file name for entry {entry} from {file_path}: reached end");
        };
        if *last != name_delimiter {
            bail!("Failed to read file name for entry {entry} from {file_path}: delimiter not found");
        }
        if name.len() == 0 {
            bail!("Failed to read file name for entry {entry} from {file_path}: empty name");
        }
        let name = name.into_iter().map(|x| *x as char).collect();
        Ok(name)
    }
    fn parse_sha(reader: &mut impl BufRead, entry: usize, file_path: &String) -> anyhow::Result<String> {
        let mut sha_buf = [0u8; 20];
        reader.read_exact(&mut sha_buf)
            .context(format!("Failed to read hash for entry {entry} from {file_path}"))?;
        let hash = hex::encode(sha_buf);
        Ok(hash)
    }
}

impl<R: BufRead> Iterator for TreeObjectIterator<R> {
    type Item = anyhow::Result<TreeEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        let res = self.parse();
        match res {
            Ok(Some(x)) => Some(Ok(x)),
            Ok(None) => None,
            Err(x) => Some(Err(x)),
        }
    }
}