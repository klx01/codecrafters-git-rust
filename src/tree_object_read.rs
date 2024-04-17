use std::io::{BufRead, Read};
use anyhow::{bail, Context};
use crate::common::{ObjectType, TreeItem};
use crate::object_read::LazyDecodedObject;

pub struct TreeObjectIterator<R: BufRead> {
    pub file_path: String,
    pub size: u64,
    reader: Option<R>,
    entry_no: usize,
    bytes_read: u64,
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
    fn parse(&mut self) -> anyhow::Result<Option<TreeItem>> {
        let res = self.parse_inner();
        match res {
            Ok(x) => Ok(x),
            Err(x) => {
                self.reader.take();
                Err(x)
            }
        }
    }
    fn parse_inner(&mut self) -> anyhow::Result<Option<TreeItem>> {
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

        let mode = Self::parse_mode(&mut sized_reader, self.entry_no, &self.file_path)?;
        let Some(mode) = mode else {
            if crate::object_read::is_end_of_reader(self.reader.take().unwrap()) {
                return Ok(None);
            } else {
                bail!("content size is larger than expected {}", self.size);
            };
        };
        let mode_len = mode.as_bytes().len();
        let mode = mode.parse::<usize>().context(format!("Failed to parse mode {} as int for entry {} from {}", mode, self.entry_no, self.file_path))?;
        let mode = mode.try_into().context(format!("Unexpected mode {} for entry {} from {}", mode, self.entry_no, self.file_path))?;

        let name = Self::parse_name(&mut sized_reader, self.entry_no, &self.file_path)?;
        let sha = Self::parse_sha(&mut sized_reader, self.entry_no, &self.file_path)?;

        let bytes_read =
            mode_len
                + 1 // delimiter ' '
                + name.as_bytes().len()
                + 1  // delimiter '\0'
                + 20; // sha
        self.bytes_read += bytes_read as u64;

        let res = TreeItem {
            mode,
            file_name: name,
            hash: sha,
        };
        Ok(Some(res))
    }
    fn parse_mode(reader: &mut impl BufRead, entry: usize, file_path: &String) -> anyhow::Result<Option<String>> {
        let mut buffer = vec![];
        let delimiter = ' ' as u8;
        reader.take(10).read_until(delimiter, &mut buffer)
            .context(format!("Failed to read mode for entry {entry} from {file_path}"))?;
        let Some((last, mode)) = buffer.split_last() else {
            return Ok(None);
        };
        if *last != delimiter {
            bail!("Failed to read mode for entry {entry} from {file_path}, delimiter not found");
        }
        if mode.len() == 0 {
            bail!("Failed to read mode for entry {entry} from {file_path}: empty name");
        }
        let mode = mode.into_iter().map(|x| *x as char).collect();
        Ok(Some(mode))
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
    type Item = anyhow::Result<TreeItem>;

    fn next(&mut self) -> Option<Self::Item> {
        let res = self.parse();
        match res {
            Ok(Some(x)) => Some(Ok(x)),
            Ok(None) => None,
            Err(x) => Some(Err(x)),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::common::ObjectMode;
    use crate::object_read::find_and_decode_object;
    use super::*;
    
    const TEST_HASH: &'static str = "d890d6190fc4b1f7f44c19e25f0b0c8f49e74f66";
    
    #[test]
    fn test_find_and_decode_object() -> anyhow::Result<()> {
        let object_data = find_and_decode_object(TEST_HASH)?;
        assert_eq!(".git/objects/d8/90d6190fc4b1f7f44c19e25f0b0c8f49e74f66", object_data.file_path);
        assert_eq!(ObjectType::Tree, object_data.object_type);
        
        let tree = TreeObjectIterator::from_decoded_object(object_data).unwrap();
        let mut tree_items = vec![];
        for item in tree {
            let item = item?;
            tree_items.push(item);
        }
        
        let expected_tree = [
            (ObjectMode::Normal, ".gitattributes", "176a458f94e0ea5272ce67c36bf30b6be9caf623"),
            (ObjectMode::Normal, ".gitignore", "345b19aec241ec34d3e111a44ee2a14236f13856"),
            (ObjectMode::Normal, "Cargo.lock", "498c8453062b1453c64c9b891dd7ed46b457a4a7"),
            (ObjectMode::Normal, "Cargo.toml", "2e489d3f007a080a250ba3eaac6f74bf6fd0a539"),
            (ObjectMode::Normal, "README.md", "9ff5466dcbd0d17667293bed96c1f3a9d56249ec"),
            (ObjectMode::Normal, "codecrafters.yml", "92afff5da6cd68e4861d1af9b0a1f92e2681a51e"),
            (ObjectMode::Executable, "my_git.sh", "877637cc56451a09af0ad17d3e48a42eda08a1c9"),
            (ObjectMode::Tree, "src", "08558b512b6590a262ed6afc8825d4a9592eed1e"),
            (ObjectMode::Executable, "your_git.sh", "92a25908ea9a3f2e1e55da59e6e4ccef25ddbd62"),
        ];
        let expected_tree = expected_tree
            .into_iter()
            .map(|x| TreeItem{mode: x.0, file_name: x.1.to_string(), hash: x.2.to_string()})
            .collect::<Vec<_>>();
        
        assert_eq!(expected_tree, tree_items);
        Ok(())
    }
}
