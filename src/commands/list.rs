use crate::client::{convert_to_string, LocalConfigClient, RemoteConfigClient};
use crate::config::{read_config};
use anyhow::Result;
use colored::Colorize;

use itertools::Itertools;

use std::path::Path;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct ListOpt {
    // Config path
    #[structopt(short, long, default_value = "./isuconf.yaml")]
    pub config: String,
}

pub async fn list(opt: ListOpt) -> Result<()> {
    let config = read_config(&opt.config).await?;
    let mut remote_client = RemoteConfigClient::new(&config.remote).await?;
    let local_client = LocalConfigClient::new(&config.local);

    for server in &config.remote.servers {
        println!(
            "{}",
            format!("{}@{}", config.remote.user.as_str(), server.name())
                .as_str()
                .purple()
        );
        for target in &config.targets {
            let exist = remote_client.exists(&server.name(), target).await?;
            if !exist {
                let real_path = convert_to_string(&remote_client.real_path(
                    &server.name(),
                    target,
                    Path::new(""),
                )?)?;
                println!("  {}", real_path.as_str().red());
                continue;
            }
            let paths = {
                let mut paths = remote_client
                    .file_relative_paths(&server.name(), target)
                    .await?;
                paths.append(
                    &mut local_client
                        .file_relative_paths(&server.name(), target)
                        .await?,
                );
                paths.iter().unique().cloned().collect_vec()
            };
            for path in &paths {
                let real_path =
                    convert_to_string(&remote_client.real_path(&server.name(), target, path)?)?;
                if remote_client
                    .exists_relative_path(&server.name(), target, path)
                    .await?
                {
                    println!("  {}", real_path.as_str());
                } else {
                    println!("  {}", real_path.as_str().red());
                }
            }
        }
    }

    println!("{}", "local".purple());
    for server in &config.remote.servers {
        for target in &config.targets {
            if target.shared {
                continue;
            }
            let exist = local_client.exists(&server.name(), target).await?;
            if !exist {
                let real_path = convert_to_string(&local_client.real_path(
                    &server.name(),
                    target,
                    Path::new(""),
                )?)?;
                println!("  {}", real_path.as_str().red());
                continue;
            }
            let paths = {
                let mut paths = remote_client
                    .file_relative_paths(&server.name(), target)
                    .await?;
                paths.append(
                    &mut local_client
                        .file_relative_paths(&server.name(), target)
                        .await?,
                );
                paths.iter().unique().cloned().collect_vec()
            };
            for path in &paths {
                let real_path =
                    convert_to_string(&local_client.real_path(&server.name(), target, path)?)?;
                if local_client
                    .exists_relative_path(&server.name(), target, path)
                    .await?
                {
                    println!("  {}", real_path.as_str());
                } else {
                    println!("  {}", real_path.as_str().red());
                }
            }
        }
    }
    if let Some(server) = &config.remote.servers.get(0) {
        for target in &config.targets {
            if !target.shared {
                continue;
            }
            let exist = local_client.exists(&server.name(), target).await?;
            if !exist {
                let real_path = convert_to_string(&local_client.real_path(
                    &server.name(),
                    target,
                    Path::new(""),
                )?)?;
                println!("  {}", real_path.as_str().red());
                continue;
            }
            let paths = {
                let mut paths = remote_client
                    .file_relative_paths(&server.name(), target)
                    .await?;
                paths.append(
                    &mut local_client
                        .file_relative_paths(&server.name(), target)
                        .await?,
                );
                paths.iter().unique().cloned().collect_vec()
            };
            for path in &paths {
                let real_path =
                    convert_to_string(&local_client.real_path(&server.name(), target, path)?)?;
                if local_client
                    .exists_relative_path(&server.name(), target, path)
                    .await?
                {
                    println!("  {}", real_path.as_str());
                } else {
                    println!("  {}", real_path.as_str().red());
                }
            }
        }
    }

    remote_client.close().await?;

    Ok(())
}
