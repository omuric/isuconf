use crate::client::{convert_to_string, join_path};
use crate::config::{RemoteConfig, TargetConfig};
use anyhow::{Context, Result, anyhow, ensure};
use openssh::{KnownHosts, Session, SessionBuilder, Stdio};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::time::timeout;

// Embedded in the binary; no script needs to be installed on the remote server.
const WRITE_FILE_SCRIPT: &str = include_str!("remote/write_file.sh");

fn checked_output(output: Output, program: &str) -> Result<Vec<u8>> {
    ensure!(
        output.status.success(),
        "Failed execute {program}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn expand_path(path: &Path, home: &str) -> Result<String> {
    let path = convert_to_string(path)?;
    if path == "~" {
        Ok(home.to_owned())
    } else if let Some(suffix) = path.strip_prefix("~/") {
        Ok(format!("{home}/{suffix}"))
    } else if path.starts_with('/') || path.starts_with("./") {
        Ok(path)
    } else {
        // Prevent relative paths beginning with '-' or '(' from becoming find options.
        Ok(format!("./{path}"))
    }
}

fn parse_file_paths(bytes: Vec<u8>, root: &str) -> Result<Vec<PathBuf>> {
    let output = String::from_utf8(bytes).context("Remote filename is not UTF-8")?;
    output
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let relative = Path::new(s)
                .strip_prefix(root)
                .with_context(|| format!("Remote path is outside target: {s}"))?;
            crate::common::validate_relative_path(relative)?;
            Ok(relative.to_path_buf())
        })
        .collect()
}

pub struct RemoteConfigClient {
    sessions: HashMap<String, Session>,
    homes: HashMap<String, String>,
}

impl RemoteConfigClient {
    pub async fn new(config: &RemoteConfig) -> Result<Self> {
        let mut sessions = HashMap::new();
        let mut homes = HashMap::new();
        for server in &config.servers {
            let mut builder = SessionBuilder::default();
            builder.known_hosts_check(KnownHosts::Add);
            if let Some(identity) = &config.identity {
                builder.keyfile(identity);
            }
            let session = timeout(
                Duration::from_secs(config.timeout.unwrap_or(5)),
                builder.connect(format!("ssh://{}@{}", config.user, server.host)),
            )
            .await
            .map_err(|_| anyhow!("Timeout connect to {}@{}", config.user, server.host))??;
            let home = checked_output(
                session.command("printenv").arg("HOME").output().await?,
                "printenv",
            )?;
            let home = String::from_utf8(home)?.trim_end_matches('\n').to_owned();
            ensure!(home.starts_with('/'), "Remote HOME must be absolute");
            homes.insert(server.name(), home);
            sessions.insert(server.name(), session);
        }
        Ok(Self { sessions, homes })
    }

    fn remote_session(&self, server: &str) -> Result<&Session> {
        self.sessions
            .get(server)
            .with_context(|| format!("Not found connection. (server={server})"))
    }

    fn command<'a>(
        &'a self,
        server: &str,
        program: &str,
        sudo: bool,
    ) -> Result<openssh::Command<'a>> {
        let session = self.remote_session(server)?;
        let mut command = session.command(if sudo { "sudo" } else { program });
        if sudo {
            command.args(["-n", "--", program]);
        }
        Ok(command)
    }

    async fn remote_command(
        &self,
        server: &str,
        program: &str,
        args: &[&str],
        sudo: bool,
    ) -> Result<Vec<u8>> {
        let output = self
            .command(server, program, sudo)?
            .args(args)
            .output()
            .await?;
        checked_output(output, program)
    }

    fn expanded_path(
        &self,
        server: &str,
        target: &TargetConfig,
        relative: &Path,
    ) -> Result<String> {
        let home = self
            .homes
            .get(server)
            .context("Missing remote home directory")?;
        expand_path(&self.real_path(server, target, relative)?, home)
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
        let path = self.expanded_path(server, target, relative)?;
        let output = self
            .command(server, "test", target.sudo)?
            .args(["-e", &path])
            .output()
            .await?;
        match output.status.code() {
            Some(0) => Ok(true),
            // Distinguish a missing file from permission/sudo errors (also exit 1).
            Some(1) if output.stderr.is_empty() => Ok(false),
            _ => {
                checked_output(output, "test")?;
                Ok(false)
            }
        }
    }

    pub async fn file_relative_paths(
        &self,
        server: &str,
        target: &TargetConfig,
    ) -> Result<Vec<PathBuf>> {
        if !self.exists(server, target).await? {
            return Ok(vec![]);
        }
        let root = self.expanded_path(server, target, Path::new(""))?;
        let bytes = self
            .remote_command(
                server,
                "find",
                &[&root, "(", "-type", "f", "-o", "-type", "l", ")", "-print0"],
                target.sudo,
            )
            .await?;
        parse_file_paths(bytes, &root)
    }

    pub fn real_path(
        &self,
        _server: &str,
        target: &TargetConfig,
        relative: &Path,
    ) -> Result<PathBuf> {
        Ok(join_path(Path::new(&target.path), relative))
    }

    pub async fn len(&self, server: &str, target: &TargetConfig, relative: &Path) -> Result<u64> {
        let path = self.expanded_path(server, target, relative)?;
        let bytes = self
            .remote_command(
                server,
                "stat",
                &["-L", "-c", "%s", "--", &path],
                target.sudo,
            )
            .await?;
        String::from_utf8(bytes)?
            .trim()
            .parse()
            .context("Failed to parse stat result")
    }

    pub async fn get(
        &self,
        server: &str,
        target: &TargetConfig,
        relative: &Path,
    ) -> Result<Vec<u8>> {
        let path = self.expanded_path(server, target, relative)?;
        // Stream directly over SSH, including sudo reads. No plaintext /tmp copy.
        self.remote_command(server, "cat", &["--", &path], target.sudo)
            .await
    }

    pub async fn create(
        &self,
        server: &str,
        target: &TargetConfig,
        relative: &Path,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let path = self.expanded_path(server, target, relative)?;
        if let Some(parent) = Path::new(&path).parent() {
            self.remote_command(
                server,
                "mkdir",
                &["-p", "--", &convert_to_string(parent)?],
                target.sudo,
            )
            .await?;
        }
        let expected_len = bytes.len().to_string();
        let mut child = self
            .command(server, "sh", target.sudo)?
            .args(["-c", WRITE_FILE_SCRIPT, "isuconf", &path, &expected_len])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .await?;
        let mut stdin = child.stdin().take().context("Missing remote stdin")?;
        let write_result = stdin.write_all(&bytes).await;
        drop(stdin);
        let output = child.wait_with_output().await?;
        checked_output(output, "write file")?;
        write_result?;
        Ok(())
    }

    pub async fn close(&mut self) -> Result<()> {
        for (_, session) in self.sessions.drain() {
            session.close().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_preserve_whitespace_and_shell_characters() {
        let root = "/etc/example config";
        let paths = parse_file_paths(b"/etc/example config/a b\0/etc/example config/line\nbreak\0/etc/example config/'$(touch nope)'\0".to_vec(), root).unwrap();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("a b"),
                PathBuf::from("line\nbreak"),
                PathBuf::from("'$(touch nope)'")
            ]
        );
        assert_eq!(
            parse_file_paths(b"/etc/hosts\0".to_vec(), "/etc/hosts").unwrap(),
            vec![PathBuf::new()]
        );
    }

    #[test]
    fn invalid_remote_paths_are_rejected() {
        assert!(parse_file_paths(b"/elsewhere/file\0".to_vec(), "/etc").is_err());
        assert!(parse_file_paths(b"/etc/../../outside\0".to_vec(), "/etc").is_err());
        assert!(parse_file_paths(vec![255, 0], "/etc").is_err());
    }

    #[test]
    fn home_and_relative_paths_are_literal() {
        assert_eq!(
            expand_path(Path::new("~/a b'$(id)"), "/srv/user").unwrap(),
            "/srv/user/a b'$(id)"
        );
        assert_eq!(expand_path(Path::new("~"), "/root").unwrap(), "/root");
        assert_eq!(
            expand_path(Path::new("-config"), "/root").unwrap(),
            "./-config"
        );
    }

    #[test]
    fn write_script_preserves_binary_and_does_not_evaluate_paths() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        use std::process::{Command, Stdio};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a '\"$(touch INJECTED)\nfile");
        let mut child = Command::new("sh")
            .args(["-c", WRITE_FILE_SCRIPT, "isuconf"])
            .arg(&path)
            .arg("4")
            .current_dir(dir.path())
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&[0, 255, 10, 42])
            .unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(std::fs::read(&path).unwrap(), [0, 255, 10, 42]);
        assert!(!dir.path().join("INJECTED").exists());
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }
    #[test]
    fn incomplete_upload_preserves_existing_file_and_cleans_private_staging() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("staging");
        std::fs::create_dir(&staging).unwrap();
        let target = dir.path().join("config");
        std::fs::write(&target, b"original configuration").unwrap();
        for exists in [true, false] {
            if !exists {
                std::fs::remove_file(&target).unwrap();
            }
            let mut child = Command::new("sh")
                .args(["-c", WRITE_FILE_SCRIPT, "isuconf"])
                .arg(&target)
                .arg("100")
                .env("TMPDIR", &staging)
                .stdin(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(b"partial").unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("Incomplete"));
            if exists {
                assert_eq!(std::fs::read(&target).unwrap(), b"original configuration");
            } else {
                assert!(!target.exists());
            }
            assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        }
    }

    #[test]
    fn completed_upload_preserves_existing_permissions_and_symlink() {
        use std::io::Write;
        use std::os::unix::fs::{PermissionsExt, symlink};
        use std::process::{Command, Stdio};
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config");
        std::fs::write(&target, b"original").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.path().join("link");
        symlink(&target, &link).unwrap();
        let mut child = Command::new("sh")
            .args(["-c", WRITE_FILE_SCRIPT, "isuconf"])
            .arg(&link)
            .arg("7")
            .env("TMPDIR", dir.path())
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"updated").unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(std::fs::read(&target).unwrap(), b"updated");
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(link.is_symlink());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
