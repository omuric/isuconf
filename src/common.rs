use crate::config::{CliConfig, TargetConfig};
use anyhow::{anyhow, Result};
use itertools::Itertools;
use std::path::{Path, PathBuf};

pub fn join_path(parent_path: &Path, path: &Path) -> PathBuf {
    if path == Path::new("") {
        return parent_path.to_owned();
    }
    parent_path.join(path)
}

pub fn convert_to_string(path: &Path) -> Result<String> {
    path.to_owned()
        .into_os_string()
        .into_string()
        .map_err(|os_string| anyhow!("Failed to string path. (os_string={:#?})", os_string))
}

pub fn is_target_config(
    cli_config: &CliConfig,
    config: &TargetConfig,
    target_config_path: &str,
) -> bool {
    let mut prefixes = cli_config
        .remote
        .servers
        .iter()
        .map(|server| format!("{}/{}", cli_config.local.config_root_path, server))
        .collect_vec();
    prefixes.push(cli_config.local.config_root_path.clone());
    prefixes.push(format!("{}/", cli_config.local.config_root_path.clone()));

    for prefix in prefixes {
        if let Some(target_config_path) = target_config_path.strip_prefix(&prefix) {
            if config.path == target_config_path {
                return true;
            }
        }
    }
    if config.path == target_config_path {
        return true;
    }
    false
}
