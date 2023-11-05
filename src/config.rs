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
    pub timeout: Option<u64>,
}

#[derive(Deserialize, Clone)]
pub struct LocalConfig {
    pub config_root_path: String,
}

#[derive(Deserialize, Clone, Debug)]
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
    pub concurrency: Option<usize>,
    pub max_file_size: Option<String>,
    pub remote: RemoteConfig,
    pub local: LocalConfig,
    pub targets: Vec<TargetConfig>,
}

impl CliConfig {
    pub fn max_file_size(&self) -> Result<u64> {
        Ok(self
            .max_file_size
            .as_ref()
            .map(|max_file_size| {
                parse_size::parse_size(max_file_size).ok().with_context(|| {
                    format!("Invalid max_file_size. (max_file_size={})", &max_file_size)
                })
            })
            .transpose()?
            .unwrap_or(300 * 1024))
    }
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
