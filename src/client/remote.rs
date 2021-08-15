use crate::client::{convert_to_string, join_path};
use crate::config::{RemoteConfig, TargetConfig};
use anyhow::{anyhow, Context, Result};
use chrono::Local;
use itertools::Itertools;
use openssh::{KnownHosts, Session, SessionBuilder};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct RemoteConfigClient {
    config: RemoteConfig,
    sessions: HashMap<String, Session>,
}

impl RemoteConfigClient {
    pub async fn new(config: &RemoteConfig) -> Result<Self> {
        let mut sessions = HashMap::new();

        for server in &config.servers {
            let mut builder = SessionBuilder::default();
            builder.known_hosts_check(KnownHosts::Accept);
            builder.control_directory("/tmp");
            let session = builder
                .connect(format!("ssh://{}@{}", config.user, server))
                .await?;

            sessions.insert(server.clone(), session);
        }

        let client = RemoteConfigClient {
            config: config.clone(),
            sessions,
        };

        Ok(client)
    }

    async fn remote_session(&self, server: &str) -> Result<&Session> {
        let session = self
            .sessions
            .get(&server.to_owned())
            .with_context(|| format!("Not found connection. (server={})", server))?;
        Ok(session)
    }

    async fn remote_command(&self, server: &str, command: &str, sudo: bool) -> Result<String> {
        let mut command = command.to_owned();
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
            .remote_command(server, &command, target.sudo)
            .await
            .is_ok();
        Ok(exists)
    }

    pub async fn exists_relative_path(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &Path,
    ) -> Result<bool> {
        let path = self.real_path(server, target, relative_path)?;
        let command = format!("ls {}", convert_to_string(&path)?);
        let exists = self
            .remote_command(server, &command, target.sudo)
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
        let command = format!("find {} -type f -o -type l", target.path);
        let result = self.remote_command(server, &command, target.sudo).await?;
        let paths: Result<Vec<_>, _> = result
            .split_whitespace()
            .filter_map(|s| {
                if s.is_empty() {
                    return None;
                }
                let home_prefix = format!("/home/{}", self.config.user);
                if s.starts_with(&home_prefix) {
                    return Some(s.replace(&home_prefix, "~"));
                }
                Some(s.to_owned())
            })
            .map(|s| Path::new(&s).to_owned())
            .map(|p| p.strip_prefix(&target.path).map(|path| path.to_owned()))
            .collect();
        Ok(paths?)
    }

    pub fn real_path(
        &self,
        _server: &str,
        target: &TargetConfig,
        relative_path: &Path,
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
        relative_path: &Path,
    ) -> Result<Vec<u8>> {
        let path = self.real_path(server, target, relative_path)?;
        let session = self.remote_session(server).await?;
        let mut config = vec![];

        if target.sudo {
            let tmp_path = format!("/tmp/{}", Local::now().to_rfc3339());

            self.remote_command(
                server,
                &format!("cp {} {}", convert_to_string(&path)?, tmp_path),
                true,
            )
            .await?;

            let mut remote_file = session.sftp().read_from(&tmp_path).await?;
            remote_file.read_to_end(&mut config).await?;
            remote_file.close().await?;

            self.remote_command(server, &format!("rm {}", tmp_path), true)
                .await?;
        } else {
            let mut path = convert_to_string(&path)?;
            if path.starts_with('~') {
                path = path.replacen('~', format!("/home/{}", self.config.user).as_str(), 1);
            }
            let mut remote_file = session.sftp().read_from(&path).await?;
            remote_file.read_to_end(&mut config).await?;
            remote_file.close().await?;
        }

        Ok(config)
    }

    pub async fn create(
        &self,
        server: &str,
        target: &TargetConfig,
        relative_path: &Path,
        config_bytes: Vec<u8>,
    ) -> Result<()> {
        let path = self.real_path(server, target, relative_path)?;
        let session = self.remote_session(server).await?;

        if target.sudo {
            let tmp_path = format!("/tmp/{}", Local::now().to_rfc3339());
            let mut remote_file = session.sftp().write_to(&tmp_path).await?;
            remote_file.write_all(&config_bytes).await?;
            remote_file.close().await?;

            if let Some(parent) = path.parent() {
                self.remote_command(
                    server,
                    &format!("mkdir -p {}", convert_to_string(&parent.to_owned())?),
                    true,
                )
                .await?;
            }

            self.remote_command(
                server,
                &format!("cp {} {}", tmp_path, convert_to_string(&path)?),
                true,
            )
            .await?;

            self.remote_command(server, &format!("rm {}", tmp_path), true)
                .await?;
        } else {
            let mut path = convert_to_string(&path)?;
            if path.starts_with('~') {
                path = path.replacen('~', format!("/home/{}", self.config.user).as_str(), 1);
            }
            let mut remote_file = session.sftp().write_to(path).await?;
            remote_file.write_all(&config_bytes).await?;
            remote_file.close().await?;
        }

        Ok(())
    }

    pub async fn close(&mut self) -> Result<()> {
        let servers = self.sessions.keys().cloned().collect_vec();
        for server in servers {
            let session = self
                .sessions
                .remove(&server)
                .with_context(|| format!("Not found session. (server={})", &server))?;
            session.close().await?;
        }
        Ok(())
    }
}
