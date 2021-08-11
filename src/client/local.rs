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

    fn parent_path(&self, server: &str, target: &TargetConfig) -> PathBuf {
        let path = Path::new(&self.config.config_root_path);
        if !target.only {
            return path.join(server);
        }
        path.to_owned()
    }

    fn path(&self, server: &str, target: &TargetConfig) -> Result<PathBuf> {
        let target_path = Path::new(&target.path);
        let path = self
            .parent_path(server, target)
            .join(if target_path.is_absolute() {
                target_path.strip_prefix("/")?
            } else {
                target_path
            })
            .to_owned();
        Ok(path)
    }

    pub async fn exists(&self, server: &str, target: &TargetConfig) -> Result<bool> {
        Ok(self.path(server, target)?.exists())
    }

    pub async fn exists_relative_path(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &PathBuf,
    ) -> Result<bool> {
        Ok(self.real_path(server, target, relative_path)?.exists())
    }

    pub async fn file_relative_paths(
        &self,
        server: &str,
        target: &TargetConfig,
    ) -> Result<Vec<PathBuf>> {
        if !self.exists(server, target).await? {
            return Ok(vec![]);
        }
        let path = self.path(server, target)?;
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
        relative_path: &PathBuf,
    ) -> Result<PathBuf> {
        Ok(join_path(&self.path(server, target)?, relative_path))
    }

    pub async fn get(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &PathBuf,
    ) -> Result<String> {
        let path = self.real_path(server, target, relative_path)?;
        Ok(fs::read_to_string(path).await?)
    }

    pub async fn create(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &PathBuf,
        config: String,
    ) -> Result<()> {
        let path = self.real_path(server, target, relative_path)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).await?;
        }
        fs::write(path, config).await?;
        Ok(())
    }
}
