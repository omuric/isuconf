use std::process::Command;

#[test]
fn help_and_version() {
    let help = Command::new(env!("CARGO_BIN_EXE_isuconf"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for name in ["pull", "push", "ssh", "ssh-config"] {
        assert!(help.contains(name));
    }
    let version = Command::new(env!("CARGO_BIN_EXE_isuconf"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(
        String::from_utf8(version.stdout)
            .unwrap()
            .contains(env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn zero_concurrency_returns_error_without_ssh() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("isuconf.yaml");
    std::fs::write(&config, "concurrency: 0\nremote:\n  servers: []\n  user: test\nlocal:\n  config_root_path: ./config\ntargets: []\n").unwrap();
    for command in ["pull", "push"] {
        let output = Command::new(env!("CARGO_BIN_EXE_isuconf"))
            .args([command, "--config"])
            .arg(&config)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("concurrency"));
        assert!(!error.contains("panicked"));
    }
}
