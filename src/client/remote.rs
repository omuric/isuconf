use crate::client::{convert_to_string, join_path};
use crate::config::{RemoteConfig, TargetConfig};
use anyhow::{anyhow, Context, Result};
use chrono::Local;
use itertools::Itertools;
use openssh::{KnownHosts, Session, SessionBuilder};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

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
            if let Some(identity) = &config.identity {
                builder.keyfile(identity);
            }
            builder.control_directory("/tmp");
            let session = timeout(
                Duration::from_secs(config.timeout.unwrap_or(5)),
                builder.connect(format!("ssh://{}@{}", config.user, server.host)),
            )
            .await
            .map_err(|_| anyhow!("Timeout connect to {}@{}", config.user, server.host))??;

            sessions.insert(server.name(), session);
        }

        let client = RemoteConfigClient {
            config: config.clone(),
            sessions,
        };

        Ok(client)
    }

    async fn remote_session(&self, server_name: &str) -> Result<&Session> {
        let server_name = server_name;
        let session = self
            .sessions
            .get(server_name)
            .with_context(|| format!("Not found connection. (server={})", &server_name))?;
        Ok(session)
    }

    async fn remote_command(&self, server_name: &str, command: &str, sudo: bool) -> Result<String> {
        let mut command = command.to_owned();
        if sudo {
            command = format!("sudo sh -c \"{}\"", command);
        }

        let output = self
            .remote_session(server_name)
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

    pub async fn exists(&self, server_name: &str, target: &TargetConfig) -> Result<bool> {
        let command = format!("ls {}", target.path);
        let exists = self
            .remote_command(server_name, &command, target.sudo)
            .await
            .is_ok();
        Ok(exists)
    }

    pub async fn exists_relative_path(
        &self,
        server_name: &str,
        target: &TargetConfig,
        relative_path: &Path,
    ) -> Result<bool> {
        let path = self.real_path(server_name, target, relative_path)?;
        let command = format!("ls {}", convert_to_string(&path)?);
        let exists = self
            .remote_command(server_name, &command, target.sudo)
            .await
            .is_ok();
        Ok(exists)
    }

    pub async fn file_relative_paths(
        &self,
        server_name: &str,
        target: &TargetConfig,
    ) -> Result<Vec<PathBuf>> {
        if !self.exists(server_name, target).await? {
            return Ok(vec![]);
        }
        let command = format!("find {} -type f -o -type l", target.path);
        let result = self
            .remote_command(server_name, &command, target.sudo)
            .await?;
        let paths: Result<Vec<_>, _> = result
            .split_whitespace()
            .filter_map(|s| {
                if s.is_empty() {
                    return None;
                }
                let home_prefix = format!("/home/{}", self.config.user);
                if target.path.starts_with('~') && s.starts_with(&home_prefix) {
                    return Some(s.replacen(&home_prefix, "~", 1));
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
        _server_name: &str,
        target: &TargetConfig,
        relative_path: &Path,
    ) -> Result<PathBuf> {
        Ok(join_path(Path::new(&target.path), relative_path))
    }

    pub async fn len(
        &self,
        server_name: &str,
        target: &TargetConfig,
        relative_path: &Path,
    ) -> Result<u64> {
        let path = self.real_path(server_name, target, relative_path)?;
        let mut path = convert_to_string(&path)?;

        if !target.sudo && path.starts_with('~') {
            path = path.replacen('~', format!("/home/{}", self.config.user).as_str(), 1);
        }

        self.remote_command(server_name, &format!("stat -c %s {}", path), target.sudo)
            .await?
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("Failed to parse stat result."))
    }

    pub async fn get(
        &self,
        server_name: &str,
        target: &TargetConfig,
        relative_path: &Path,
    ) -> Result<Vec<u8>> {
        let path = self.real_path(server_name, target, relative_path)?;
        let session = self.remote_session(server_name).await?;
        let mut config = vec![];

        if target.sudo {
            let tmp_path = format!("/tmp/{}", Local::now().to_rfc3339());

            self.remote_command(
                server_name,
                &format!("cp {} {}", convert_to_string(&path)?, tmp_path),
                true,
            )
            .await?;

            self.remote_command(server_name, &format!("chmod 644 {}", tmp_path), true)
                .await?;

            let mut remote_file = session.sftp().read_from(&tmp_path).await?;
            remote_file.read_to_end(&mut config).await?;
            remote_file.close().await?;

            self.remote_command(server_name, &format!("rm {}", tmp_path), true)
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
        server_name: &str,
        target: &TargetConfig,
        relative_path: &Path,
        config_bytes: Vec<u8>,
    ) -> Result<()> {
        let path = self.real_path(server_name, target, relative_path)?;
        let session = self.remote_session(server_name).await?;

        if target.sudo {
            let tmp_path = format!("/tmp/{}", Local::now().to_rfc3339());
            let mut remote_file = session.sftp().write_to(&tmp_path).await?;
            remote_file.write_all(&config_bytes).await?;
            remote_file.close().await?;

            if let Some(parent) = path.parent() {
                self.remote_command(
                    server_name,
                    &format!("mkdir -p {}", convert_to_string(parent)?),
                    true,
                )
                .await?;
            }

            self.remote_command(
                server_name,
                &format!("cp {} {}", tmp_path, convert_to_string(&path)?),
                true,
            )
            .await?;

            self.remote_command(server_name, &format!("rm {}", tmp_path), true)
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
