use crate::client::join_path;
use crate::config::{LocalConfig, TargetConfig};
use anyhow::Result;
use async_recursion::async_recursion;
use std::path::{Path, PathBuf};
use tokio::fs;

#[async_recursion]
async fn file_paths_in_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut file_paths = vec![];
    if dir.is_dir() {
        let mut dir = fs::read_dir(dir).await?;
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                file_paths.append(&mut file_paths_in_dirs(&path).await?);
            } else {
                file_paths.push(path);
            }
        }
    }
    Ok(file_paths)
}

pub struct LocalConfigClient {
    config: LocalConfig,
}

impl LocalConfigClient {
    pub fn new(config: &LocalConfig) -> Self {
        LocalConfigClient {
            config: config.clone(),
        }
    }

    fn parent_path(&self, server_name: &str, target: &TargetConfig) -> PathBuf {
        let path = Path::new(&self.config.config_root_path);
        if !target.shared {
            return path.join(server_name);
        }
        path.to_owned()
    }

    fn path(&self, server_name: &str, target: &TargetConfig) -> Result<PathBuf> {
        let target_path = Path::new(&target.path);
        let path = self
            .parent_path(server_name, target)
            .join(if target_path.is_absolute() {
                target_path.strip_prefix("/")?
            } else {
                target_path
            });
        Ok(path)
    }

    pub async fn exists(&self, server_name: &str, target: &TargetConfig) -> Result<bool> {
        Ok(self.path(server_name, target)?.exists())
    }

    pub async fn exists_relative_path(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &Path,
    ) -> Result<bool> {
        Ok(self.real_path(server, target, relative_path)?.exists())
    }

    pub async fn file_relative_paths(
        &self,
        server_name: &str,
        target: &TargetConfig,
    ) -> Result<Vec<PathBuf>> {
        if !self.exists(server_name, target).await? {
            return Ok(vec![]);
        }
        let path = self.path(server_name, target)?;
        let paths = if path.is_file() {
            vec![path.clone()]
        } else if path.is_dir() {
            file_paths_in_dirs(&path).await?
        } else {
            vec![]
        };
        let paths: Result<Vec<_>, _> = paths
            .iter()
            .map(|p| p.strip_prefix(&path).map(|path| path.to_owned()))
            .collect();
        Ok(paths?)
    }

    pub fn real_path(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &Path,
    ) -> Result<PathBuf> {
        Ok(join_path(&self.path(server, target)?, relative_path))
    }

    pub async fn get(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &Path,
    ) -> Result<Vec<u8>> {
        let path = self.real_path(server, target, relative_path)?;
        Ok(fs::read(path).await?)
    }

    pub async fn create(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &Path,
        config_bytes: Vec<u8>,
    ) -> Result<()> {
        let path = self.real_path(server, target, relative_path)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).await?;
        }
        fs::write(path, config_bytes).await?;
        Ok(())
    }
}
