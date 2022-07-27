use anyhow::{Context, Result};
use serde_derive::Deserialize;
use tokio::fs;

fn default_as_true() -> bool {
    true
}

fn default_as_false() -> bool {
    false
}

#[derive(Deserialize, Clone)]
pub struct ServerConfig {
    pub alias: Option<String>,
    pub host: String,
}

impl ServerConfig {
    pub fn name(&self) -> String {
        self.alias.to_owned().unwrap_or_else(|| self.host.clone())
    }
}

#[derive(Deserialize, Clone)]
pub struct RemoteConfig {
    pub servers: Vec<ServerConfig>,
    pub user: String,
    pub identity: Option<String>,
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
    pub shared: bool,
}

#[derive(Deserialize, Clone)]
pub struct CliConfig {
    pub remote: RemoteConfig,
    pub local: LocalConfig,
    pub targets: Vec<TargetConfig>,
}

pub async fn read_config(config_path: &str) -> Result<CliConfig> {
    let json = fs::read_to_string(&config_path).await.with_context(|| {
        format!(
            "Not found configuration file. (config_path={})",
            &config_path
        )
    })?;
    let config = serde_yaml::from_str(&json)
        .with_context(|| format!("Invalid config file. (config_path={})", config_path))?;
    Ok(config)
}
