use std::cmp::Ordering;
use std::fs;
use std::io::{BufReader, Cursor, Read, Write};
use anyhow::{bail, Context};
use crate::common::{GIT_PATH, make_object_header, ObjectMode, ObjectType, TreeItem};
use crate::object_write::{calc_object_hash, create_blob_object, NewObjectData, write_object_file};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub fn create_tree_object(dir_path: PathBuf) -> anyhow::Result<NewObjectData<impl Read>> {
    create_tree_object_internal(dir_path, false)
}

pub fn create_and_write_tree_object(dir_path: PathBuf) -> anyhow::Result<String> {
    let object = create_tree_object_internal(dir_path, true)?;
    write_object_file(object)
}

fn create_tree_object_internal(dir_path: PathBuf, write_files: bool) -> anyhow::Result<NewObjectData<impl Read>> {
    let tree = get_tree_entries(dir_path, write_files)?;
    let tree_vec = tree_into_bytes_raw(&tree)?;
    let tree_vec_len = tree_vec.len();
    let header = make_object_header(ObjectType::Tree, tree_vec_len as u64);
    let hash = calc_object_hash(BufReader::new(&tree_vec[..]), &header).context("Failed to calc hash")?;
    let res = NewObjectData {
        hash,
        header,
        data_reader: Cursor::new(tree_vec),
    };
    Ok(res)
}

fn get_tree_entries(dir_path: PathBuf, write_files: bool) -> anyhow::Result<Vec<TreeItem>> {
    let dir_files = fs::read_dir(&dir_path).context(format!("Failed to read dir {}", dir_path.to_str().unwrap()))?;

    let mut result = vec![];
    for dir_entry in dir_files {
        let dir_entry = dir_entry.context(format!("Some weird error while reading dir entry name in {}", dir_path.to_str().unwrap()))?;
        let file_name_os = dir_entry.file_name();
        let Some(file_name) = file_name_os.to_str() else {
            bail!("Failed to convert file name to str {file_name_os:?}");
        };

        let file_name = file_name.to_string();
        if file_name == GIT_PATH {
            // todo: what is the correct way to handle .git dirs and files that are not at the top level?
            continue;
        }

        let path = dir_entry.path();
        let meta = path.metadata().context(format!("Failed to read metadata for {file_name}"))?;
        if meta.is_symlink() {
            bail!("Handling symlinks is not implemented yet! {file_name}");
        }

        let tree_item = if meta.is_dir() {
            let mode = ObjectMode::Tree;
            let object = create_tree_object_internal(path, write_files)?;
            
            let hash = if write_files {
                write_object_file(object)?
            } else {
                object.hash
            };
            TreeItem {mode, file_name, hash}
        } else if meta.is_file() {
            let mode = if meta.permissions().mode() & 0o111 != 0 { 
                ObjectMode::Executable 
            } else { 
                ObjectMode::Normal 
            };
            let object = create_blob_object(path, write_files)?;
            
            let hash = if write_files {
                write_object_file(object)?
            } else {
                object.hash
            };
            TreeItem {mode, file_name, hash}
        } else {
            bail!("found path is neither dir nor file {file_name}");
        };
        result.push(tree_item);
    }

    result.sort_by(tree_item_sort);

    Ok(result)
}

fn tree_item_sort(left: &TreeItem, right: &TreeItem) -> Ordering {
    left.file_name.partial_cmp(&right.file_name).unwrap()
}

fn tree_into_bytes_raw(tree: &[TreeItem]) -> anyhow::Result<Vec<u8>> {
    let mut res = vec![];
    tree_into_writer_raw(tree, &mut res)?;
    Ok(res)
}

fn tree_into_writer_raw(tree: &[TreeItem], writer: &mut impl Write) -> anyhow::Result<()> {
    for item in tree {
        let hex = hex::decode(&item.hash).context(format!("failed to decode hash {}", item.hash))?;
        write!(
            writer,
            "{} {}\0",
            item.mode as usize,
            item.file_name,
        )?;
        writer.write(&hex)?;
    }
    Ok(())
}
