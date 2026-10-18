//! Run with tests/ssh.sh; this test requires a disposable local SSH server.
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::Command;

fn read_source(path: &Path, sudo: bool) -> Vec<u8> {
    if !sudo {
        return std::fs::read(path).unwrap();
    }
    let output = Command::new("sudo")
        .args(["-n", "cat", "--"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    output.stdout
}

fn run(config: &Path, command: &str, dry_run: bool) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_isuconf"));
    cmd.args([command, "--config"]).arg(config);
    if dry_run {
        cmd.arg("--dry-run");
    }
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
#[ignore = "requires disposable local SSH server; run tests/ssh.sh"]
fn ssh_roundtrip_sudo_dry_run_and_literal_filenames() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote space '$(touch INJECTED)");
    std::fs::create_dir(&remote).unwrap();
    let filename = "line\nbreak \"'$(touch INJECTED)";
    let source = remote.join(filename);
    let initial = vec![0, 255, 10, 42];
    std::fs::write(&source, &initial).unwrap();
    let local = dir.path().join("local");
    let config = dir.path().join("isuconf.yaml");
    let user = std::env::var("USER").unwrap();
    let identity = std::env::var("ISUCONF_TEST_IDENTITY").expect("run tests/ssh.sh");
    let host = std::env::var("ISUCONF_TEST_HOST").unwrap();
    let test_sudo = std::env::var("ISUCONF_TEST_NO_SUDO").as_deref() != Ok("1");
    for sudo in [false, true] {
        if sudo && !test_sudo {
            continue;
        }
        // All fields are serialized so quotes and newlines retain their literal meaning.
        let yaml = format!(
            "remote:\n  servers:\n    - alias: test\n      host: {}\n  user: {}\n  identity: {}\nlocal:\n  config_root_path: {}\ntargets:\n  - path: {}\n    sudo: {sudo}\n    shared: true\n",
            serde_saphyr::to_string(&host).unwrap().trim(),
            serde_saphyr::to_string(&user).unwrap().trim(),
            serde_saphyr::to_string(&identity).unwrap().trim(),
            serde_saphyr::to_string(&local.to_str().unwrap())
                .unwrap()
                .trim(),
            serde_saphyr::to_string(&remote.to_str().unwrap())
                .unwrap()
                .trim()
        );
        std::fs::write(&config, yaml).unwrap();
        std::fs::write(&source, &initial).unwrap();
        if local.exists() {
            std::fs::remove_dir_all(&local).unwrap();
        }
        if sudo {
            assert!(
                Command::new("sudo")
                    .args(["-n", "chown", "root:root", "--"])
                    .arg(&source)
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(
                Command::new("sudo")
                    .args(["-n", "chmod", "600", "--"])
                    .arg(&source)
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(std::fs::read(&source).is_err());
        }
        run(&config, "pull", true);
        assert!(!local.exists());
        run(&config, "pull", false);
        let destination = local.join(remote.strip_prefix("/").unwrap()).join(filename);
        assert_eq!(std::fs::read(&destination).unwrap(), initial);
        assert!(run(&config, "pull", false).contains("synced"));
        std::fs::write(&destination, b"updated\0\xff").unwrap();
        run(&config, "push", true);
        assert_eq!(read_source(&source, sudo), initial);
        run(&config, "push", false);
        assert_eq!(read_source(&source, sudo), b"updated\0\xff");
        let new_relative = if sudo {
            "new sudo directory/new file"
        } else {
            "new directory/new file"
        };
        let new_path = destination.parent().unwrap().join(new_relative);
        std::fs::create_dir_all(new_path.parent().unwrap()).unwrap();
        std::fs::write(&new_path, b"created").unwrap();
        run(&config, "push", false);
        let created = remote.join(new_relative);
        // Read as the login user even when sudo created the root-owned file.
        assert_eq!(std::fs::read(&created).unwrap(), b"created");
        assert_eq!(
            std::fs::metadata(&created).unwrap().permissions().mode() & 0o777,
            0o644
        );
        let link_name = if sudo {
            "linked sudo directory"
        } else {
            "linked directory"
        };
        let linked_directory = destination.parent().unwrap().join(link_name);
        symlink(Path::new(new_relative).parent().unwrap(), &linked_directory).unwrap();
        run(&config, "push", false);
        assert_eq!(
            read_source(&remote.join(link_name).join("new file"), sudo),
            b"created"
        );
        if sudo {
            assert!(
                std::fs::read(&source).is_err(),
                "Existing private files must remain private"
            );
        }
    }
    if test_sudo {
        assert!(
            Command::new("sudo")
                .args(["-n", "chown", "-R", &user, "--"])
                .arg(&remote)
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(!Path::new("INJECTED").exists());
}

#[tokio::test]
#[ignore = "requires a changed SSH host key; run tests/ssh.sh"]
async fn rejects_changed_host_key() {
    use isuconf::client::RemoteConfigClient;
    use isuconf::config::{RemoteConfig, ServerConfig};
    if std::env::var("ISUCONF_TEST_EXPECT_HOST_KEY_CHANGE").as_deref() != Ok("1") {
        return;
    }
    let config = RemoteConfig {
        servers: vec![ServerConfig {
            alias: None,
            host: std::env::var("ISUCONF_TEST_HOST").unwrap(),
        }],
        user: std::env::var("USER").unwrap(),
        identity: Some(std::env::var("ISUCONF_TEST_IDENTITY").unwrap()),
        timeout: Some(5),
    };
    let error = match RemoteConfigClient::new(&config).await {
        Ok(mut client) => {
            client.close().await.unwrap();
            panic!("Changed host key was accepted");
        }
        Err(error) => error,
    };
    assert!(
        format!("{error:#}").contains("HOST IDENTIFICATION HAS CHANGED"),
        "{error:#}"
    );
}
