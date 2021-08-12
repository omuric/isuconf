use crate::config::{CliConfig, TargetConfig};
use anyhow::{anyhow, Result};
use itertools::Itertools;
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

pub fn is_target_config(
    config: &CliConfig,
    target: &TargetConfig,
    target_config_path: &Option<String>,
) -> bool {
    if let Some(t1) = &target_config_path {
        let mut prefixes = config
            .remote
            .servers
            .iter()
            .map(|server| format!("{}/{}", config.local.config_root_path, server))
            .collect_vec();
        prefixes.push(config.local.config_root_path.clone());
        for prefix in prefixes {
            if let Some(t2) = t1.strip_prefix(&prefix) {
                if &target.path == t2 {
                    return true;
                }
            }
        }
        if &target.path == t1 {
            return true;
        }
    }
    false
}
