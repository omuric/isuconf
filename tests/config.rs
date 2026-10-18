use isuconf::config::{CliConfig, read_config};

const CONFIG: &str = "remote:\n  servers:\n    - host: example.test\n  user: isucon\nlocal:\n  config_root_path: ./config\ntargets:\n  - path: /etc/hosts\n";

#[test]
fn defaults_and_size_limits() {
    let config: CliConfig = serde_saphyr::from_str(CONFIG).unwrap();
    config.validate().unwrap();
    assert_eq!(config.max_file_size().unwrap(), 300 * 1024);
    let target = &config.targets[0];
    assert!(target.push && target.pull);
    assert!(!target.sudo && !target.shared);
    assert_eq!(config.remote.servers[0].name(), "example.test");
    let mut config = config;
    config.max_file_size = Some("2 MiB".into());
    assert_eq!(config.max_file_size().unwrap(), 2 * 1024 * 1024);
    config.max_file_size = Some("invalid".into());
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn invalid_concurrency_fails_before_connecting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, format!("concurrency: 0\n{CONFIG}")).unwrap();
    let result = read_config(path.to_str().unwrap()).await;
    assert!(result.err().unwrap().to_string().contains("concurrency"));
}

#[test]
fn duplicate_server_names_are_rejected() {
    let mut config: CliConfig = serde_saphyr::from_str(CONFIG).unwrap();
    config.remote.servers.push(config.remote.servers[0].clone());
    assert!(config.validate().is_err());
}

#[test]
fn repository_example_is_valid() {
    let config: CliConfig = serde_saphyr::from_str(include_str!("../isuconf.yaml")).unwrap();
    config.validate().unwrap();
}
