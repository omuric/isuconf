use crate::common::validate_relative_path;
use crate::config::{LocalConfig, TargetConfig};
use anyhow::{Context, Result, ensure};
use cap_std::ambient_authority;
use cap_std::fs::{
    Dir, DirBuilder, DirBuilderExt, OpenOptions, OpenOptionsExt, Permissions, PermissionsExt,
};
use std::collections::HashSet;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};

fn file_paths_in_dirs(
    root: &Dir,
    path: &Path,
    files: &mut Vec<PathBuf>,
    active_directories: &mut HashSet<PathBuf>,
) -> Result<()> {
    let canonical = root.canonicalize(path)?;
    if !active_directories.insert(canonical.clone()) {
        return Ok(());
    }
    for entry in root.read_dir(path)? {
        let entry = entry?;
        let child = path.join(entry.file_name());
        // Resolve links through the capability root, never through ambient paths.
        let kind = root.metadata(&child)?.file_type();
        if kind.is_dir() {
            file_paths_in_dirs(root, &child, files, active_directories)?;
        } else if kind.is_file() {
            files.push(child);
        }
    }
    active_directories.remove(&canonical);
    Ok(())
}

pub struct LocalConfigClient {
    config: LocalConfig,
}

impl LocalConfigClient {
    pub fn new(config: &LocalConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    fn relative_path(
        &self,
        server: &str,
        target: &TargetConfig,
        relative: &Path,
    ) -> Result<PathBuf> {
        validate_relative_path(relative)?;
        let target_path = Path::new(&target.path);
        let target_path = target_path.strip_prefix("/").unwrap_or(target_path);
        validate_relative_path(target_path)?;
        let mut path = PathBuf::new();
        if !target.shared {
            validate_relative_path(Path::new(server))?;
            ensure!(!server.is_empty(), "Server name must not be empty");
            path.push(server);
        }
        path.push(target_path);
        if !relative.as_os_str().is_empty() {
            path.push(relative);
        }
        Ok(path)
    }

    // cap-std resolves every operation relative to this directory handle, including
    // symlinks, instead of checking a path and reopening it through ambient access.
    async fn with_root<T, F>(&self, create: bool, operation: F) -> Result<Option<T>>
    where
        T: Send + 'static,
        F: FnOnce(Dir) -> Result<T> + Send + 'static,
    {
        let root = self.config.config_root_path.clone();
        tokio::task::spawn_blocking(move || {
            if create {
                std::fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(&root)?;
            }
            let root = match Dir::open_ambient_dir(&root, ambient_authority()) {
                Ok(root) => root,
                Err(error) if !create && error.kind() == ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            operation(root).map(Some)
        })
        .await?
    }

    pub async fn exists(&self, server: &str, target: &TargetConfig) -> Result<bool> {
        self.exists_relative_path(server, target, Path::new(""))
            .await
    }

    pub async fn exists_relative_path(
        &self,
        server: &str,
        target: &TargetConfig,
        relative: &Path,
    ) -> Result<bool> {
        let path = self.relative_path(server, target, relative)?;
        Ok(self
            .with_root(false, move |root| Ok(root.try_exists(path)?))
            .await?
            .unwrap_or(false))
    }

    pub async fn file_relative_paths(
        &self,
        server: &str,
        target: &TargetConfig,
    ) -> Result<Vec<PathBuf>> {
        let path = self.relative_path(server, target, Path::new(""))?;
        Ok(self
            .with_root(false, move |root| {
                let metadata = match root.metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == ErrorKind::NotFound => return Ok(vec![]),
                    Err(error) => return Err(error.into()),
                };
                if metadata.is_file() {
                    return Ok(vec![PathBuf::new()]);
                }
                if !metadata.is_dir() {
                    return Ok(vec![]);
                }
                let mut files = Vec::new();
                file_paths_in_dirs(&root, &path, &mut files, &mut HashSet::new())?;
                files
                    .into_iter()
                    .map(|file| Ok(file.strip_prefix(&path)?.to_path_buf()))
                    .collect()
            })
            .await?
            .unwrap_or_default())
    }

    pub fn real_path(
        &self,
        server: &str,
        target: &TargetConfig,
        relative: &Path,
    ) -> Result<PathBuf> {
        Ok(Path::new(&self.config.config_root_path)
            .join(self.relative_path(server, target, relative)?))
    }

    pub async fn len(&self, server: &str, target: &TargetConfig, relative: &Path) -> Result<u64> {
        let path = self.relative_path(server, target, relative)?;
        self.with_root(false, move |root| Ok(root.metadata(path)?.len()))
            .await?
            .context("Local config directory does not exist")
    }

    pub async fn get(
        &self,
        server: &str,
        target: &TargetConfig,
        relative: &Path,
    ) -> Result<Vec<u8>> {
        let path = self.relative_path(server, target, relative)?;
        self.with_root(false, move |root| Ok(root.read(path)?))
            .await?
            .context("Local config directory does not exist")
    }

    pub async fn create(
        &self,
        server: &str,
        target: &TargetConfig,
        relative: &Path,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let path = self.relative_path(server, target, relative)?;
        ensure!(path.file_name().is_some(), "A local file path is required");
        self.with_root(true, move |root| {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                root.create_dir_with(parent, DirBuilder::new().recursive(true).mode(0o700))?;
            }
            let mut options = OpenOptions::new();
            options.write(true).create(true).mode(0o600);
            let mut file = root.open_with(&path, &options)?;
            // Tighten existing files before replacing their content as well.
            file.set_permissions(Permissions::from_mode(0o600))?;
            file.set_len(0)?;
            file.write_all(&bytes)?;
            Ok(())
        })
        .await?;
        Ok(())
    }
}
