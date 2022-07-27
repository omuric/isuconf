use anyhow::Result;
use colored::Colorize;
use futures::future;
use isuconf::client::{convert_to_string, is_target_config, LocalConfigClient, RemoteConfigClient};
use isuconf::config::{read_config, CliConfig, ServerConfig, TargetConfig};
use itertools::Itertools;
use std::collections::HashMap;
use std::path::Path;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct ListOpt {
    // Config path
    #[structopt(short, long, default_value = "./isuconf.yaml")]
    config: String,
}

async fn list(opt: ListOpt) -> Result<()> {
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

#[derive(StructOpt, Debug)]
struct PullOpt {
    // Config path
    #[structopt(short, long, default_value = "./isuconf.yaml")]
    config: String,
    // Dry run
    #[structopt(short, long)]
    dry_run: bool,
    // Target config
    #[structopt(name = "TARGET_CONFIG_PATH")]
    target_config_path: Option<String>,
}

async fn pull_target_from_server(
    local_client: &LocalConfigClient,
    remote_client: &RemoteConfigClient,
    opt: &PullOpt,
    server: &ServerConfig,
    target: &TargetConfig,
) -> Result<()> {
    let paths = remote_client
        .file_relative_paths(&server.name(), target)
        .await?;

    for path in &paths {
        let remote_config = remote_client.get(&server.name(), target, path).await?;
        let real_path =
            convert_to_string(&local_client.real_path(&server.name(), target, path)?)?;
        if local_client
            .exists_relative_path(&server.name(), target, path)
            .await?
        {
            let local_config = local_client.get(&server.name(), target, path).await?;
            if remote_config == local_config {
                println!("no difference {}", real_path);
                continue;
            } else {
                println!("found the difference {}", real_path);
            }
            if !opt.dry_run {
                local_client
                    .create(&server.name(), target, path, remote_config)
                    .await?;
            }
            println!("update {}", real_path.as_str().green());
        } else {
            if !opt.dry_run {
                local_client
                    .create(&server.name(), target, path, remote_config)
                    .await?;
            }
            println!("create {}", real_path.as_str().green());
        }
    }

    Ok(())
}

async fn exists_remote(
    remote_client: &RemoteConfigClient,
    server: &ServerConfig,
    target: &TargetConfig,
) -> Result<(String, bool)> {
    let exists = remote_client.exists(&server.name(), target).await?;
    Ok((server.name(), exists))
}

async fn pull(opt: PullOpt) -> Result<()> {
    let cli_config = read_config(&opt.config).await?;

    let mut remote_client = RemoteConfigClient::new(&cli_config.remote).await?;
    let local_client = LocalConfigClient::new(&cli_config.local);

    for target in &cli_config.targets {
        if let Some(target_config_path) = &opt.target_config_path {
            if !is_target_config(&cli_config, target, target_config_path) {
                continue;
            }
        }
        if !target.pull {
            println!("skip {}", target.path.as_str().purple());
            continue;
        }
        println!("pull {}", target.path.as_str().purple());
        let mut tasks = vec![];

        for (_, server) in cli_config.remote.servers.iter().enumerate() {
            tasks.push(exists_remote(&remote_client, server, target));
        }

        let exists_map = future::try_join_all(tasks)
            .await?
            .into_iter()
            .collect::<HashMap<String, bool>>();

        let mut tasks = vec![];

        for (idx, server) in cli_config.remote.servers.iter().enumerate() {
            let exists = exists_map.get(&server.name()).cloned().unwrap_or(false);

            if !exists {
                let real_path = convert_to_string(&remote_client.real_path(
                    &server.name(),
                    target,
                    Path::new(""),
                )?)?;
                println!(
                    "not found {}@{}:{}",
                    cli_config.remote.user,
                    &server.name(),
                    real_path.as_str().red(),
                );
                continue;
            }
            if idx >= 1 && target.shared {
                continue;
            }

            tasks.push(pull_target_from_server(
                &local_client,
                &remote_client,
                &opt,
                server,
                target,
            ));
        }

        future::try_join_all(tasks).await?;
    }

    remote_client.close().await?;

    Ok(())
}

#[derive(StructOpt, Debug)]
struct PushOpt {
    // Config path
    #[structopt(short, long, default_value = "./isuconf.yaml")]
    config: String,
    // Dry run
    #[structopt(short, long)]
    dry_run: bool,
    // Target config
    #[structopt(name = "TARGET_CONFIG_PATH")]
    target_config_path: Option<String>,
}

async fn push_target_to_server(
    local_client: &LocalConfigClient,
    remote_client: &RemoteConfigClient,
    opt: &PushOpt,
    cli_config: &CliConfig,
    server: &ServerConfig,
    target: &TargetConfig,
) -> Result<()> {
    let paths = local_client
        .file_relative_paths(&server.name(), target)
        .await?;
    for path in &paths {
        let local_config = local_client.get(&server.name(), target, path).await?;
        let real_path =
            convert_to_string(&remote_client.real_path(&server.name(), target, path)?)?;
        if remote_client
            .exists_relative_path(&server.name(), target, path)
            .await?
        {
            let remote_config = remote_client.get(&server.name(), target, path).await?;

            if local_config == remote_config {
                println!(
                    "no difference {}@{}:{}",
                    cli_config.remote.user,
                    &server.name(),
                    real_path
                );
                continue;
            } else {
                println!(
                    "found the difference {}@{}:{}",
                    cli_config.remote.user,
                    &server.name(),
                    real_path
                );
            }
            if !opt.dry_run {
                remote_client
                    .create(&server.name(), target, path, local_config)
                    .await?;
            }
            println!(
                "update {}@{}:{}",
                cli_config.remote.user,
                &server.name(),
                real_path.as_str().green()
            );
        } else {
            if !opt.dry_run {
                remote_client
                    .create(&server.name(), target, path, local_config)
                    .await?;
            }
            println!(
                "create {}@{}:{}",
                cli_config.remote.user,
                &server.name(),
                real_path.as_str().green()
            );
        }
    }

    Ok(())
}
async fn exists_local(
    local_client: &LocalConfigClient,
    server: &ServerConfig,
    target: &TargetConfig,
) -> Result<(String, bool)> {
    let exists = local_client.exists(&server.name(), target).await?;
    Ok((server.name(), exists))
}

async fn push(opt: PushOpt) -> Result<()> {
    let cli_config = read_config(&opt.config).await?;

    let local_client = LocalConfigClient::new(&cli_config.local);
    let mut remote_client = RemoteConfigClient::new(&cli_config.remote).await?;

    for target in &cli_config.targets {
        if let Some(target_config_path) = &opt.target_config_path {
            if !is_target_config(&cli_config, target, target_config_path) {
                continue;
            }
        }
        if !target.push {
            println!("skip {}", target.path.as_str().purple());
            continue;
        }
        println!("push {}", target.path.as_str().purple());

        let mut tasks = vec![];

        for server in &cli_config.remote.servers {
            tasks.push(exists_local(&local_client, server, target));
        }

        let exists_map = future::try_join_all(tasks)
            .await?
            .into_iter()
            .collect::<HashMap<String, bool>>();

        let mut tasks = vec![];

        for server in &cli_config.remote.servers {
            let exists = exists_map.get(&server.name()).cloned().unwrap_or(false);

            if !exists {
                let real_path = convert_to_string(&local_client.real_path(
                    &server.name(),
                    target,
                    Path::new(""),
                )?)?;
                println!("not found {}", real_path.as_str().red());
                if target.shared {
                    break;
                } else {
                    continue;
                }
            }

            tasks.push(push_target_to_server(
                &local_client,
                &remote_client,
                &opt,
                &cli_config,
                server,
                target,
            ));
        }

        future::try_join_all(tasks).await?;
    }

    remote_client.close().await?;

    Ok(())
}

#[derive(StructOpt, Debug)]
struct SshOpt {
    // Config path
    #[structopt(short, long, default_value = "./isuconf.yaml")]
    config: String,
    // Server name
    #[structopt(name = "SERVER_NAME")]
    server_name: String,
}

async fn ssh(opt: SshOpt) -> Result<()> {
    let cli_config = read_config(&opt.config).await?;
    let server_name = opt.server_name.to_owned();

    let remote = cli_config.remote;

    let server = remote
        .servers
        .iter()
        .find(|server| server.name() == server_name);

    if let Some(server) = server {
        print!("ssh {}@{}", remote.user, server.host);

        if let Some(identity) = &remote.identity {
            print!(" -i {}", identity)
        }

        println!();
    }

    Ok(())
}

#[derive(StructOpt, Debug)]
struct SshConfigOpt {
    // Config path
    #[structopt(short, long, default_value = "./isuconf.yaml")]
    config: String,
}

async fn ssh_config(opt: SshConfigOpt) -> Result<()> {
    let cli_config = read_config(&opt.config).await?;
    let remote = &cli_config.remote;

    for server in &remote.servers {
        println!("Host {}", server.name());
        println!("  HostName {}", &server.host);
        println!("  User {}", &remote.user);
        if let Some(identity) = &remote.identity {
            print!("  IdentityFile {}", identity)
        }
        println!();
    }
    Ok(())
}

#[derive(StructOpt, Debug)]
#[structopt(name = "isuconf")]
enum Opt {
    /// View config list
    List(ListOpt),
    /// Pull configs from remote
    Pull(PullOpt),
    /// Push configs to remote
    Push(PushOpt),
    /// Helper command for ssh
    Ssh(SshOpt),
    /// Helper command for ssh config
    SshConfig(SshConfigOpt),
}

#[tokio::main]
async fn main() -> Result<()> {
    let opt: Opt = Opt::from_args();
    match opt {
        Opt::List(opt) => list(opt).await,
        Opt::Pull(opt) => pull(opt).await,
        Opt::Push(opt) => push(opt).await,
        Opt::Ssh(opt) => ssh(opt).await,
        Opt::SshConfig(opt) => ssh_config(opt).await,
    }
}
