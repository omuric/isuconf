use crate::client::{convert_to_string, is_target_config, LocalConfigClient, RemoteConfigClient};
use crate::config::{read_config, TargetConfig};
use anyhow::Result;
use colored::Colorize;
use futures::StreamExt;
use itertools::Itertools;
use std::cmp::max;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use tokio::time::Instant;

#[derive(StructOpt, Debug)]
pub struct PullOpt {
    // Config path
    #[structopt(short, long, default_value = "./isuconf.yaml")]
    pub config: String,
    // Dry run
    #[structopt(short, long)]
    pub dry_run: bool,
    // Target config
    #[structopt(name = "TARGET_CONFIG_PATH")]
    pub target_config_path: Option<String>,
}

#[derive(Debug)]
pub enum PullRemoteTaskState {
    Skip,
    NotExists,
    Progress,
    Synced,
    FoundDiff,
    FoundNewFile,
}

impl PullRemoteTaskState {
    fn message(&self, file_message: &str) -> String {
        let icon = match self {
            PullRemoteTaskState::NotExists => "✕".red(),
            PullRemoteTaskState::Skip => " ".normal(),
            PullRemoteTaskState::Progress => " ".normal(),
            PullRemoteTaskState::Synced => " ".normal(),
            PullRemoteTaskState::FoundDiff => " ".normal(),
            PullRemoteTaskState::FoundNewFile => " ".normal(),
        };
        let message = match self {
            PullRemoteTaskState::NotExists => "not exists".normal(),
            PullRemoteTaskState::Skip => "skip".normal(),
            PullRemoteTaskState::Progress => "".normal(),
            PullRemoteTaskState::Synced => "synced 📌".normal(),
            PullRemoteTaskState::FoundDiff => "found diff 🔍️".normal(),
            PullRemoteTaskState::FoundNewFile => "found new file 🔍".normal(),
        };
        let file_message = match self {
            PullRemoteTaskState::NotExists => file_message.red(),
            PullRemoteTaskState::Skip => file_message.normal(),
            PullRemoteTaskState::Progress => file_message.normal(),
            PullRemoteTaskState::Synced => file_message.normal(),
            PullRemoteTaskState::FoundDiff => file_message.normal(),
            PullRemoteTaskState::FoundNewFile => file_message.normal(),
        };
        format!("▕  {} ▕  {}  ▕  {} ", file_message, icon, message)
    }
}

#[derive(Debug)]
pub struct PullRemoteTask {
    relative_path: PathBuf,
    server_name: String,
    state: PullRemoteTaskState,
}

#[derive(Debug)]
pub enum PullLocalTaskState {
    Progress,
    Create,
    Update,
    Skip,
}

impl PullLocalTaskState {
    fn message(&self, file_message: &str) -> String {
        let icon = match self {
            PullLocalTaskState::Progress => "◰".green(),
            PullLocalTaskState::Create => "✓".green(),
            PullLocalTaskState::Update => "✓".green(),
            PullLocalTaskState::Skip => "-".normal(),
        };
        let message = match self {
            PullLocalTaskState::Progress => "".normal(),
            PullLocalTaskState::Create => "create 📦️️".normal(),
            PullLocalTaskState::Update => "update ✏️️".normal(),
            PullLocalTaskState::Skip => "skip".purple(),
        };
        let file_message = match self {
            PullLocalTaskState::Progress => file_message.normal(),
            PullLocalTaskState::Create => file_message.bright_green(),
            PullLocalTaskState::Update => file_message.bright_green(),
            PullLocalTaskState::Skip => file_message.normal(),
        };

        format!("▕  {} ▕  {}  ▕  {} ", file_message, icon, message)
    }
}

#[derive(Debug)]
pub struct PullLocalTask {
    path: PathBuf,
}

#[derive(Debug)]
pub struct PullTask {
    remote: PullRemoteTask,
    local: PullLocalTask,
    target: TargetConfig,
}

pub struct PullTaskResult {
    messages: Vec<String>,
}

struct PullContext {
    local_client: LocalConfigClient,
    remote_client: RemoteConfigClient,
    remote_user: String,
    dry_run: bool,
    file_message_len_max: usize,
}

async fn execute_pull_task(task: PullTask, ctx: &PullContext) -> Result<PullTaskResult> {
    let remote_path = &ctx.remote_client.real_path(
        &task.remote.server_name,
        &task.target,
        &task.remote.relative_path,
    )?;

    let file_message = format!(
        "{}@{}:{}",
        &ctx.remote_user,
        &task.remote.server_name,
        convert_to_string(remote_path)?
    );
    let file_message_len_diff = ctx.file_message_len_max - file_message.len();

    let remote_file_message = format!("{}{}", file_message, " ".repeat(file_message_len_diff));

    match &task.remote.state {
        PullRemoteTaskState::Skip | PullRemoteTaskState::NotExists => {
            return Ok(PullTaskResult {
                messages: vec![task.remote.state.message(&remote_file_message)],
            });
        }
        _ => {}
    }

    let remote_config = ctx
        .remote_client
        .get(
            &task.remote.server_name,
            &task.target,
            &task.remote.relative_path,
        )
        .await?;

    let local_path = convert_to_string(&task.local.path)?;
    let file_message = format!("└─> {}", local_path);
    let file_message_len_diff = ctx.file_message_len_max - file_message.len() + 4;

    let local_file_message = format!("{}{}", file_message, " ".repeat(file_message_len_diff));

    if ctx
        .local_client
        .exists_relative_path(
            &task.remote.server_name,
            &task.target,
            &task.remote.relative_path,
        )
        .await?
    {
        let local_config = ctx
            .local_client
            .get(
                &task.remote.server_name,
                &task.target,
                &task.remote.relative_path,
            )
            .await?;

        if remote_config == local_config {
            let remote_state = PullRemoteTaskState::Synced;
            let remote_message = remote_state.message(&remote_file_message);
            return Ok(PullTaskResult {
                messages: vec![remote_message],
            });
        }
        if !ctx.dry_run {
            ctx.local_client
                .create(
                    &task.remote.server_name,
                    &task.target,
                    &task.remote.relative_path,
                    remote_config,
                )
                .await?;
        }
        let remote_state = PullRemoteTaskState::FoundDiff;
        let remote_message = remote_state.message(&remote_file_message);
        let local_state = PullLocalTaskState::Update;
        let local_message = local_state.message(&local_file_message);
        Ok(PullTaskResult {
            messages: vec![remote_message, local_message],
        })
    } else {
        if !ctx.dry_run {
            ctx.local_client
                .create(
                    &task.remote.server_name,
                    &task.target,
                    &task.remote.relative_path,
                    remote_config,
                )
                .await?;
        }
        let remote_state = PullRemoteTaskState::FoundNewFile;
        let remote_message = remote_state.message(&remote_file_message);
        let local_state = PullLocalTaskState::Create;
        let local_message = local_state.message(&local_file_message);
        Ok(PullTaskResult {
            messages: vec![remote_message, local_message],
        })
    }
}

pub async fn pull(opt: PullOpt) -> Result<()> {
    let config = read_config(&opt.config).await?;

    let begin_time = Instant::now();

    let remote_client = RemoteConfigClient::new(&config.remote).await?;
    let local_client = LocalConfigClient::new(&config.local);

    let mut tasks = vec![];

    for target in &config.targets {
        if let Some(target_config_path) = &opt.target_config_path {
            if !is_target_config(&config, target, target_config_path) {
                continue;
            }
        }

        for (idx, server) in config.remote.servers.iter().enumerate() {
            if idx >= 1 && target.shared {
                continue;
            }
            let local_path = local_client.real_path(&server.name(), target, Path::new(""))?;
            if !target.pull {
                tasks.push(PullTask {
                    remote: PullRemoteTask {
                        relative_path: PathBuf::new(),
                        server_name: server.name(),
                        state: PullRemoteTaskState::Skip,
                    },
                    local: PullLocalTask {
                        path: local_path.to_owned(),
                    },
                    target: target.to_owned(),
                });
                continue;
            }
            let paths = remote_client
                .file_relative_paths(&server.name(), target)
                .await?;
            if paths.is_empty() {
                tasks.push(PullTask {
                    remote: PullRemoteTask {
                        relative_path: PathBuf::new(),
                        server_name: server.name(),
                        state: PullRemoteTaskState::NotExists,
                    },
                    local: PullLocalTask {
                        path: local_path.to_owned(),
                    },
                    target: target.to_owned(),
                });
                continue;
            }
            for path in &paths {
                let local_path = local_client.real_path(&server.name(), target, path)?;
                tasks.push(PullTask {
                    remote: PullRemoteTask {
                        relative_path: path.to_owned(),
                        server_name: server.name(),
                        state: PullRemoteTaskState::Progress,
                    },
                    local: PullLocalTask { path: local_path },
                    target: target.to_owned(),
                })
            }
        }
    }

    let remote_prefix_len_max = tasks
        .iter()
        .map(|target| {
            let remote_path = &remote_client.real_path(
                &target.remote.server_name,
                &target.target,
                &target.remote.relative_path,
            )?;

            let prefix = format!(
                "{}@{}:{}",
                &config.remote.user,
                &target.remote.server_name,
                convert_to_string(remote_path)?
            );
            Ok(prefix.len())
        })
        .collect::<Result<Vec<usize>>>()?
        .into_iter()
        .max()
        .unwrap_or(0);

    let local_prefix_len_max = tasks
        .iter()
        .map(|target| {
            let prefix = format!("└─> {}", convert_to_string(&target.local.path)?);
            Ok(prefix.len() + 4)
        })
        .collect::<Result<Vec<usize>>>()?
        .into_iter()
        .max()
        .unwrap_or(0);

    let mut ctx = PullContext {
        local_client,
        remote_client,
        remote_user: config.remote.user,
        dry_run: opt.dry_run,
        file_message_len_max: max(remote_prefix_len_max, local_prefix_len_max),
    };

    for sub_tasks in tasks
        .into_iter()
        .chunks(config.concurrency.unwrap_or(10))
        .into_iter()
    {
        let mut stream = futures::stream::FuturesOrdered::new();

        for task in sub_tasks {
            stream.push(execute_pull_task(task, &ctx));
        }

        while let Some(result) = stream.next().await {
            let result = result?;
            for message in result.messages {
                println!("{}", message);
            }
        }
    }

    ctx.remote_client.close().await?;

    let end_time = Instant::now();

    let elapsed = end_time - begin_time;

    println!(
        "  Finished pull 🚀 [{}.{}s] ",
        elapsed.as_secs(),
        elapsed.subsec_millis()
    );

    Ok(())
}
