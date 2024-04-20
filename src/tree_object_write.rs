use std::cmp::Ordering;
use std::{cmp, fs};
use std::io::Write;
use anyhow::{bail, Context};
use crate::common::{GIT_PATH, ObjectMode, ObjectType, TreeItem};
use crate::object_write::{hash_blob, hash_object};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub fn hash_tree(dir_path: &PathBuf, write_files: bool) -> anyhow::Result<Option<String>> {
    let dir_entries = get_dir_entries_sorted(dir_path)?;
    if dir_entries.len() == 0 {
        return Ok(None);
    }

    let mut tree_data = vec![];
    let tree_iterator = TreeIterator { inner: dir_entries.into_iter(), write_files };
    for tree_item in tree_iterator {
        let tree_item = tree_item?;
        let hex = hex::decode(&tree_item.hash).context(format!("failed to decode hash {}", tree_item.hash))?;
        write!(tree_data, "{} ", tree_item.mode)?;
        // todo: use vec operations instead of write
        tree_data.write(tree_item.file_name.as_encoded_bytes())?;
        tree_data.write(&[0])?;
        tree_data.write(&hex)?;
    }
    let tree_data_len = tree_data.len();
    if tree_data_len == 0 {
        return Ok(None);
    }

    let hash = hash_object(tree_data.as_slice(), ObjectType::Tree, tree_data_len as u64, write_files)?;
    Ok(Some(hash))
}

struct TreeIterator<I: Iterator<Item = (PathBuf, ObjectMode)>> {
    inner: I,
    write_files: bool,
}
impl<I: Iterator<Item = (PathBuf, ObjectMode)>> TreeIterator<I> {
    fn next_inner(&mut self) -> anyhow::Result<Option<TreeItem>> {
        loop {
            let Some((path, mode)) = self.inner.next() else {
                return Ok(None);
            };
            let item = self.get_tree_item(path, mode)?;
            if item.is_some() {
                return Ok(item);
            }
            // else it was an empty dir, which is skipped, and we yield next entry
        }
    }
    fn get_tree_item(&mut self, path: PathBuf, mode: ObjectMode) -> anyhow::Result<Option<TreeItem>> {
        let hash = match mode {
            ObjectMode::Tree => hash_tree(&path, self.write_files)?,
            ObjectMode::Normal | ObjectMode::Executable => Some(hash_blob(&path, self.write_files)?),
            ObjectMode::Symlink => bail!("Handling symlinks is not implemented yet! {}", path.display()),
        };
        let Some(hash) = hash else {
            return Ok(None);
        };
        let file_name = path.file_name().unwrap().to_os_string();
        let tree_item = TreeItem {mode, file_name, hash};
        Ok(Some(tree_item))
    }
}
impl<I: Iterator<Item = (PathBuf, ObjectMode)>> Iterator for TreeIterator<I> {
    type Item = anyhow::Result<TreeItem>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next_inner();
        match next {
            Ok(Some(x)) => Some(Ok(x)),
            Ok(None) => None,
            Err(x) => Some(Err(x)),
        }
    }
}

fn get_dir_entries_sorted(dir_path: &PathBuf) -> anyhow::Result<Vec<(PathBuf, ObjectMode)>> {
    let dir_iterator = fs::read_dir(dir_path).context(format!("Failed to read dir {}", dir_path.to_str().unwrap()))?;
    let mut files = vec![];
    for dir_entry in dir_iterator {
        let dir_entry = dir_entry.context(format!("Some weird error while reading dir entry name in {}", dir_path.to_str().unwrap()))?;

        let path = dir_entry.path();
        let meta = path.metadata().context(format!("Failed to read metadata for {}", path.display()))?;
        if meta.is_symlink() {
            bail!("Handling symlinks is not implemented yet! {}", path.display());
        }
        if path.file_name().unwrap().as_encoded_bytes() == GIT_PATH.as_bytes() {
            // todo: what is the correct way to handle .git dirs and files that are not at the top level?
            continue;
        }

        let meta = path.metadata().context(format!("Failed to read metadata for {}", path.display()))?;
        if meta.is_symlink() {
            bail!("Handling symlinks is not implemented yet! {}", path.display());
        }
        let mode = if meta.is_dir() {
            ObjectMode::Tree
        } else if meta.is_file() {
            if meta.permissions().mode() & 0o111 != 0 {
                ObjectMode::Executable
            } else {
                ObjectMode::Normal
            }
        } else {
            bail!("found path is neither dir nor file {}", path.display());
        };
        files.push((path, mode));
    }
    files.sort_unstable_by(entry_sort);
    Ok(files)
}

fn entry_sort(left: &(PathBuf, ObjectMode), right: &(PathBuf, ObjectMode)) -> Ordering {
    let left_name = left.0.file_name().unwrap().as_encoded_bytes();
    let right_name = right.0.file_name().unwrap().as_encoded_bytes();
    let common_len = cmp::min(left_name.len(), right_name.len());
    let (left_base, left_rest) = left_name.split_at(common_len);
    let (right_base, right_rest) = right_name.split_at(common_len);
    let base_cmp = left_base.cmp(right_base);
    if base_cmp != Ordering::Equal {
        return base_cmp;
    }
    let left_next = match left_rest.first() {
        Some(x) => x,
        None => if left.1 == ObjectMode::Tree { &b'/' } else { &0 },
    };
    let right_next = match right_rest.first() {
        Some(x) => x,
        None => if right.1 == ObjectMode::Tree { &b'/' } else { &0 },
    };
    left_next.cmp(right_next)
}

#[cfg(test)]
mod test {
    use std::ffi::OsString;
    use crate::common::init_test;
    use crate::object_read::find_and_decode_object;
    use crate::tree_object_read::TreeObjectIterator;
    use super::*;

    #[test]
    fn test_hash_tree() -> anyhow::Result<()> {
        init_test()?;
        let path = PathBuf::from("empty");
        fs::create_dir_all(&path)?;
        let hash = hash_tree(&path, true)?;
        assert!(hash.is_none());

        let path = PathBuf::from(".");
        let hash = hash_tree(&path, true)?.unwrap();
        assert_eq!("0b70d742c267c707ebd81d8968fc2e696a9e2edb", hash);

        let read = find_and_decode_object(&hash)?;
        assert_eq!(ObjectType::Tree, read.object_type);
        assert_eq!(218, read.size as usize);
        assert_eq!(".git/objects/0b/70d742c267c707ebd81d8968fc2e696a9e2edb", read.file_path);

        let read = TreeObjectIterator::from_decoded_object(read).unwrap();
        let tree = read.map(|x| x.unwrap()).collect::<Vec<_>>();

        let expected_tree = [
            (ObjectMode::Tree, "data", "9f7ac4df44c5f17df5cbe6aff66c257a541544cd"),
            (ObjectMode::Normal, "order", "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"),
            (ObjectMode::Normal, "order.txt", "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"),
            (ObjectMode::Tree, "order_dir.dir", "417c01c8795a35b8e835113a85a5c0c1c77f67fb"),
            (ObjectMode::Normal, "order_dir.txt", "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"),
            (ObjectMode::Tree, "order_dir", "417c01c8795a35b8e835113a85a5c0c1c77f67fb"),
        ];
        let expected_tree = expected_tree
            .into_iter()
            .map(|x| TreeItem{mode: x.0, file_name: OsString::from(x.1), hash: x.2.to_string()})
            .collect::<Vec<_>>();
        assert_eq!(expected_tree, tree);

        Ok(())
    }
}
