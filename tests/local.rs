use isuconf::client::LocalConfigClient;
use isuconf::config::{LocalConfig, TargetConfig};
use std::path::{Path, PathBuf};

fn target(shared: bool) -> TargetConfig {
    TargetConfig {
        path: "/etc/example config".into(),
        push: true,
        pull: true,
        sudo: false,
        shared,
    }
}

#[tokio::test]
async fn local_roundtrip_and_server_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let client = LocalConfigClient::new(&LocalConfig {
        config_root_path: dir.path().to_str().unwrap().into(),
    });
    let target = target(false);
    let relative = Path::new("nested/a b\nfile");
    assert!(!client.exists("one", &target).await.unwrap());
    client
        .create("one", &target, relative, vec![0, 255, 42])
        .await
        .unwrap();
    assert_eq!(
        client.get("one", &target, relative).await.unwrap(),
        [0, 255, 42]
    );
    assert_eq!(client.len("one", &target, relative).await.unwrap(), 3);
    assert_eq!(
        client.file_relative_paths("one", &target).await.unwrap(),
        vec![PathBuf::from(relative)]
    );
    assert!(!client.exists("two", &target).await.unwrap());
    assert_eq!(
        client.real_path("one", &target, relative).unwrap(),
        dir.path().join("one/etc/example config").join(relative)
    );
}

#[tokio::test]
async fn shared_and_single_file_targets() {
    let dir = tempfile::tempdir().unwrap();
    let client = LocalConfigClient::new(&LocalConfig {
        config_root_path: dir.path().to_str().unwrap().into(),
    });
    let target = target(true);
    client
        .create("one", &target, Path::new(""), b"shared".to_vec())
        .await
        .unwrap();
    assert_eq!(
        client.get("two", &target, Path::new("")).await.unwrap(),
        b"shared"
    );
    assert_eq!(
        client.file_relative_paths("two", &target).await.unwrap(),
        vec![PathBuf::new()]
    );
}

#[tokio::test]
async fn rejects_paths_outside_the_local_root() {
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("store");
    let client = LocalConfigClient::new(&LocalConfig {
        config_root_path: store.to_str().unwrap().into(),
    });
    for relative in ["../../../outside", "/outside"] {
        assert!(
            client
                .create(
                    "one",
                    &target(true),
                    Path::new(relative),
                    b"secret".to_vec()
                )
                .await
                .is_err()
        );
    }
    let mut invalid = target(true);
    invalid.path = "/etc/../../outside".into();
    assert!(
        client
            .create("one", &invalid, Path::new(""), vec![])
            .await
            .is_err()
    );
    assert!(
        client
            .create("../outside", &target(false), Path::new(""), vec![])
            .await
            .is_err()
    );
    assert!(!store.exists());
}

#[tokio::test]
async fn rejects_escaping_symlinks_for_reads_and_writes() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("store");
    std::fs::create_dir_all(store.join("etc/example config")).unwrap();
    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    let victim = outside.join("victim");
    std::fs::write(&victim, b"original").unwrap();
    symlink(&outside, store.join("etc/example config/parent")).unwrap();
    symlink(&victim, store.join("etc/example config/file")).unwrap();
    let client = LocalConfigClient::new(&LocalConfig {
        config_root_path: store.to_str().unwrap().into(),
    });
    for relative in ["parent/victim", "file"] {
        let path = Path::new(relative);
        assert!(client.get("one", &target(true), path).await.is_err());
        assert!(client.len("one", &target(true), path).await.is_err());
        assert!(
            client
                .create("one", &target(true), path, b"overwrite".to_vec())
                .await
                .is_err()
        );
    }
    assert_eq!(std::fs::read(&victim).unwrap(), b"original");
}

#[tokio::test]
async fn pulled_files_and_new_directories_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("store");
    let client = LocalConfigClient::new(&LocalConfig {
        config_root_path: store.to_str().unwrap().into(),
    });
    let relative = Path::new("secret");
    client
        .create("one", &target(false), relative, b"secret".to_vec())
        .await
        .unwrap();
    let path = client.real_path("one", &target(false), relative).unwrap();
    for directory in [
        &store,
        &store.join("one"),
        &store.join("one/etc"),
        path.parent().unwrap(),
    ] {
        assert_eq!(
            std::fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    client
        .create("one", &target(false), relative, b"updated".to_vec())
        .await
        .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(std::fs::read(path).unwrap(), b"updated");
}

#[tokio::test]
async fn enumerates_internal_directory_links_without_following_cycles() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("etc/example config");
    std::fs::create_dir_all(folder.join("real/nested")).unwrap();
    std::fs::write(folder.join("real/nested/file.conf"), b"configuration").unwrap();
    symlink("real", folder.join("linked")).unwrap();
    symlink(".", folder.join("real/self")).unwrap();
    symlink("../..", folder.join("real/nested/ancestor")).unwrap();
    let client = LocalConfigClient::new(&LocalConfig {
        config_root_path: dir.path().to_str().unwrap().into(),
    });
    let mut files = client
        .file_relative_paths("one", &target(true))
        .await
        .unwrap();
    files.sort();
    assert_eq!(
        files,
        vec![
            PathBuf::from("linked/nested/file.conf"),
            PathBuf::from("real/nested/file.conf")
        ]
    );
    for file in files {
        assert_eq!(
            client.get("one", &target(true), &file).await.unwrap(),
            b"configuration"
        );
    }
}

#[tokio::test]
async fn directory_enumeration_rejects_links_outside_the_root() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("store");
    let folder = root.join("etc/example config");
    std::fs::create_dir_all(&folder).unwrap();
    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("secret"), b"secret").unwrap();
    symlink(&outside, folder.join("escape")).unwrap();
    let client = LocalConfigClient::new(&LocalConfig {
        config_root_path: root.to_str().unwrap().into(),
    });
    assert!(
        client
            .file_relative_paths("one", &target(true))
            .await
            .is_err()
    );
}
