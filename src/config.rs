use anyhow::{Context, Result};
use serde_derive::Deserialize;
use std::path::Path;
use tokio::fs;

fn default_as_true() -> bool {
    true
}

fn default_as_false() -> bool {
    false
}

#[derive(Deserialize, Clone)]
pub struct RemoteConfig {
    pub servers: Vec<String>,
    pub user: String,
}

#[derive(Deserialize, Clone)]
pub struct LocalConfig {
    pub config_root_path: String,
}

#[derive(Deserialize, Clone)]
pub struct TargetConfig {
    pub path: String,
    #[serde(default = "default_as_true")]
    pub push: bool,
    #[serde(default = "default_as_true")]
    pub pull: bool,
    #[serde(default = "default_as_false")]
    pub sudo: bool,
    #[serde(default = "default_as_false")]
    pub only: bool,
}

#[derive(Deserialize, Clone)]
pub struct CliConfig {
    pub remote: RemoteConfig,
    pub local: LocalConfig,
    pub targets: Vec<TargetConfig>,
}

pub async fn read_config(config_path: impl AsRef<Path>) -> Result<CliConfig> {
    let json = fs::read_to_string(config_path)
        .await
        .with_context(|| format!("Not found configuration file.",))?;
    let config = serde_yaml::from_str(&json).with_context(|| format!("Invalid config file."))?;
    Ok(config)
}
