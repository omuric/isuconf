use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

pub fn join_path(parent_path: &PathBuf, path: &PathBuf) -> PathBuf {
    if path == Path::new("") {
        return parent_path.to_owned();
    }
    parent_path.join(path)
}

pub fn convert_to_string(path: &PathBuf) -> Result<String> {
    path.to_owned()
        .into_os_string()
        .into_string()
        .map_err(|os_string| anyhow!("Failed to string path. (os_string={:#?})", os_string))
}
