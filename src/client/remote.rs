use crate::client::{convert_to_string, join_path};
use crate::config::{RemoteConfig, TargetConfig};
use anyhow::{anyhow, Context, Result};
use chrono::Local;
use openssh::{KnownHosts, Session};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub struct RemoteConfigClient {
    config: RemoteConfig,
    sessions: HashMap<String, Session>,
}

impl RemoteConfigClient {
    pub async fn new(config: &RemoteConfig) -> Result<Self> {
        let mut sessions = HashMap::new();

        for server in &config.servers {
            let session = Session::connect(
                format!("ssh://{}@{}", config.user, server),
                KnownHosts::Accept,
            )
            .await?;
            sessions.insert(server.clone(), session);
        }

        let client = RemoteConfigClient {
            config: config.clone(),
            sessions,
        };

        Ok(client)
    }

    async fn remote_session(&self, server: String) -> Result<&Session> {
        let session = self
            .sessions
            .get(&server)
            .with_context(|| format!("Not found connection. (server={})", server))?;
        Ok(session)
    }

    async fn remote_command(&self, server: String, command: String, sudo: bool) -> Result<String> {
        let mut command = command;
        if sudo {
            command = format!("sudo sh -c \"{}\"", command);
        }

        let output = self
            .remote_session(server)
            .await?
            .raw_command(&command)
            .output()
            .await?;

        let stdout = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed execute command\ncommand: {}\nstdout: {}\nstderr: {}",
                command,
                stdout,
                stderr,
            ));
        }

        Ok(stdout)
    }

    pub async fn exists(&self, server: &str, target: &TargetConfig) -> Result<bool> {
        let command = format!("ls {}", target.path);
        let exists = self
            .remote_command(server.to_owned(), command, target.sudo)
            .await
            .is_ok();
        Ok(exists)
    }

    pub async fn exists_relative_path(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &PathBuf,
    ) -> Result<bool> {
        let path = self.real_path(server, target, relative_path)?;
        let command = format!("ls {}", convert_to_string(&path)?);
        let exists = self
            .remote_command(server.to_owned(), command, target.sudo)
            .await
            .is_ok();
        Ok(exists)
    }

    pub async fn file_relative_paths(
        &self,
        server: &str,
        target: &TargetConfig,
    ) -> Result<Vec<PathBuf>> {
        if !self.exists(server, target).await? {
            return Ok(vec![]);
        }
        let command = format!("find {} -type f", target.path);
        let result = self
            .remote_command(server.to_owned(), command, target.sudo)
            .await?;
        let paths: Result<Vec<_>, _> = result
            .split_whitespace()
            .map(|s| {
                let home_prefix = format!("/home/{}", self.config.user);
                if s.starts_with(&home_prefix) {
                    return s.replace(&home_prefix, "~");
                }
                s.to_owned()
            })
            .filter_map(|s| {
                if s.is_empty() {
                    None
                } else {
                    Some(Path::new(&s).to_owned())
                }
            })
            .map(|p| p.strip_prefix(&target.path).map(|path| path.to_owned()))
            .collect();
        Ok(paths?)
    }

    pub fn real_path(
        &self,
        _server: &str,
        target: &TargetConfig,
        relative_path: &PathBuf,
    ) -> Result<PathBuf> {
        Ok(join_path(
            &Path::new(&target.path).to_owned(),
            relative_path,
        ))
    }

    pub async fn get(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &PathBuf,
    ) -> Result<String> {
        let path = self.real_path(server, target, relative_path)?;
        let command = format!("cat {}", convert_to_string(&path)?);
        let result = self
            .remote_command(server.to_owned(), command, target.sudo)
            .await?;
        Ok(result)
    }

    pub async fn create(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &PathBuf,
        config: String,
    ) -> Result<()> {
        let path = self.real_path(server, target, relative_path)?;
        let session = self.remote_session(server.to_string()).await?;

        if target.sudo {
            let tmp_path = format!("/tmp/{}", Local::now().to_rfc3339());
            let mut remote_file = session.sftp().write_to(&tmp_path).await?;
            remote_file.write_all(config.as_bytes()).await?;
            remote_file.close().await?;

            if let Some(parent) = path.parent() {
                self.remote_command(
                    server.to_owned(),
                    format!("mkdir -p {}", convert_to_string(&parent.to_owned())?),
                    true,
                )
                .await?;
            }

            self.remote_command(
                server.to_owned(),
                format!("mv {} {}", tmp_path, convert_to_string(&path)?),
                true,
            )
            .await?;
        } else {
            let mut remote_file = session.sftp().write_to(path).await?;
            remote_file.write_all(config.as_bytes()).await?;
            remote_file.close().await?;
        }

        Ok(())
    }
}