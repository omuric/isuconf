use crate::client::{convert_to_string, is_target_config, LocalConfigClient, RemoteConfigClient};
use crate::config::{read_config, TargetConfig};
use anyhow::Result;
use colored::Colorize;
use futures::StreamExt;
use itertools::Itertools;
use std::cmp::max;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct PushOpt {
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
pub enum PushLocalTaskState {
    Skip,
    NotExists,
    Progress,
    Synced,
    FoundDiff,
    FoundNewFile,
}

impl PushLocalTaskState {
    fn message(&self, file_message: &str) -> String {
        let icon = match self {
            PushLocalTaskState::NotExists => "✕".red(),
            PushLocalTaskState::Skip => " ".normal(),
            PushLocalTaskState::Progress => " ".normal(),
            PushLocalTaskState::Synced => " ".normal(),
            PushLocalTaskState::FoundDiff => " ".normal(),
            PushLocalTaskState::FoundNewFile => " ".normal(),
        };
        let message = match self {
            PushLocalTaskState::NotExists => "not exists".normal(),
            PushLocalTaskState::Skip => "skip".normal(),
            PushLocalTaskState::Progress => "".normal(),
            PushLocalTaskState::Synced => "synced 📌".normal(),
            PushLocalTaskState::FoundDiff => "found diff 🔍".normal(),
            PushLocalTaskState::FoundNewFile => "found new file 🔍".normal(),
        };
        let file_message = match self {
            PushLocalTaskState::NotExists => file_message.red(),
            PushLocalTaskState::Skip => file_message.normal(),
            PushLocalTaskState::Progress => file_message.normal(),
            PushLocalTaskState::Synced => file_message.normal(),
            PushLocalTaskState::FoundDiff => file_message.normal(),
            PushLocalTaskState::FoundNewFile => file_message.normal(),
        };
        format!("▕  {} ▕  {}  ▕  {} ", file_message, icon, message)
    }
}

#[derive(Debug)]
pub struct PushLocalTask {
    path: PathBuf,
    state: PushLocalTaskState,
    is_hidden: bool,
}

#[derive(Debug)]
pub enum PushRemoteTaskState {
    Progress,
    Create,
    Update,
    Skip,
}

impl PushRemoteTaskState {
    fn message(&self, file_message: &str) -> String {
        let icon = match self {
            PushRemoteTaskState::Progress => "◰".green(),
            PushRemoteTaskState::Create => "✓".green(),
            PushRemoteTaskState::Update => "✓".green(),
            PushRemoteTaskState::Skip => "-".normal(),
        };
        let message = match self {
            PushRemoteTaskState::Progress => "".normal(),
            PushRemoteTaskState::Create => "create 📦️️".normal(),
            PushRemoteTaskState::Update => "update ✏️️".normal(),
            PushRemoteTaskState::Skip => "skip".purple(),
        };
        let file_message = match self {
            PushRemoteTaskState::Progress => file_message.normal(),
            PushRemoteTaskState::Create => file_message.bright_green(),
            PushRemoteTaskState::Update => file_message.bright_green(),
            PushRemoteTaskState::Skip => file_message.normal(),
        };

        format!("▕  {} ▕  {}  ▕  {} ", file_message, icon, message)
    }
}

#[derive(Debug)]
pub struct PushRemoteTask {
    relative_path: PathBuf,
    server_name: String,
}

#[derive(Debug)]
pub struct PushTask {
    remote: PushRemoteTask,
    local: PushLocalTask,
    target: TargetConfig,
}

pub struct PushTaskResult {
    messages: Vec<String>,
}

struct PushContext {
    local_client: LocalConfigClient,
    remote_client: RemoteConfigClient,
    remote_user: String,
    dry_run: bool,
    file_message_len_max: usize,
}

async fn execute_push_task(task: PushTask, ctx: &PushContext) -> Result<PushTaskResult> {
    let local_path = convert_to_string(&task.local.path)?;

    let file_message = local_path;
    let file_message_len_diff = ctx.file_message_len_max - file_message.len();

    let local_file_message = format!("{}{}", file_message, " ".repeat(file_message_len_diff));

    match &task.local.state {
        PushLocalTaskState::Skip | PushLocalTaskState::NotExists => {
            return Ok(PushTaskResult {
                messages: if task.local.is_hidden {
                    vec![]
                } else {
                    vec![task.local.state.message(&local_file_message)]
                },
            })
        }
        _ => {}
    }

    let local_config = ctx
        .local_client
        .get(
            &task.remote.server_name,
            &task.target,
            &task.remote.relative_path,
        )
        .await?;

    let remote_path = &ctx.remote_client.real_path(
        &task.remote.server_name,
        &task.target,
        &task.remote.relative_path,
    )?;
    let file_message = format!(
        "└─> {}@{}:{}",
        &ctx.remote_user,
        &task.remote.server_name,
        convert_to_string(remote_path)?
    );
    let file_message_len_diff = ctx.file_message_len_max - file_message.len() + 4;

    let remote_file_message = format!("{}{}", file_message, " ".repeat(file_message_len_diff));

    if ctx
        .remote_client
        .exists_relative_path(
            &task.remote.server_name,
            &task.target,
            &task.remote.relative_path,
        )
        .await?
    {
        let remote_config = ctx
            .remote_client
            .get(
                &task.remote.server_name,
                &task.target,
                &task.remote.relative_path,
            )
            .await?;

        if local_config == remote_config {
            let local_state = PushLocalTaskState::Synced;
            let local_message = local_state.message(&local_file_message);
            return Ok(PushTaskResult {
                messages: if task.local.is_hidden {
                    vec![]
                } else {
                    vec![local_message]
                },
            });
        }
        if !ctx.dry_run {
            ctx.remote_client
                .create(
                    &task.remote.server_name,
                    &task.target,
                    &task.remote.relative_path,
                    local_config,
                )
                .await?;
        }
        let local_state = PushLocalTaskState::FoundDiff;
        let local_message = local_state.message(&local_file_message);
        let remote_state = PushRemoteTaskState::Update;
        let remote_message = remote_state.message(&remote_file_message);
        Ok(PushTaskResult {
            messages: if task.local.is_hidden {
                vec![remote_message]
            } else {
                vec![local_message, remote_message]
            },
        })
    } else {
        if !ctx.dry_run {
            ctx.remote_client
                .create(
                    &task.remote.server_name,
                    &task.target,
                    &task.remote.relative_path,
                    local_config,
                )
                .await?;
        }
        let local_state = PushLocalTaskState::FoundNewFile;
        let local_message = local_state.message(&local_file_message);
        let remote_state = PushRemoteTaskState::Create;
        let remote_message = remote_state.message(&remote_file_message);
        Ok(PushTaskResult {
            messages: if task.local.is_hidden {
                vec![remote_message]
            } else {
                vec![local_message, remote_message]
            },
        })
    }
}

pub async fn push(opt: PushOpt) -> Result<()> {
    let config = read_config(&opt.config).await?;

    let begin_time = Instant::now();

    let local_client = LocalConfigClient::new(&config.local);
    let remote_client = RemoteConfigClient::new(&config.remote).await?;

    let mut tasks = vec![];

    for target in &config.targets {
        if let Some(target_config_path) = &opt.target_config_path {
            if !is_target_config(&config, target, target_config_path) {
                continue;
            }
        }

        let mut server_names_by_path: HashMap<PathBuf, Vec<String>> = HashMap::new();

        for (idx, server) in config.remote.servers.iter().enumerate() {
            let is_hidden_local = idx >= 1 && target.shared;
            let local_path = local_client.real_path(&server.name(), target, Path::new(""))?;
            if !target.push {
                tasks.push(PushTask {
                    local: PushLocalTask {
                        path: local_path.to_owned(),
                        state: PushLocalTaskState::Skip,
                        is_hidden: is_hidden_local,
                    },
                    remote: PushRemoteTask {
                        relative_path: PathBuf::new(),
                        server_name: server.name(),
                    },
                    target: target.to_owned(),
                });
                continue;
            }
            let paths = local_client
                .file_relative_paths(&server.name(), target)
                .await?;
            if paths.is_empty() {
                tasks.push(PushTask {
                    local: PushLocalTask {
                        path: local_path.to_owned(),
                        state: PushLocalTaskState::NotExists,
                        is_hidden: is_hidden_local,
                    },
                    remote: PushRemoteTask {
                        relative_path: PathBuf::new(),
                        server_name: server.name(),
                    },
                    target: target.to_owned(),
                });
                continue;
            }

            for path in paths {
                server_names_by_path
                    .entry(path)
                    .and_modify(|server_names| server_names.push(server.name()))
                    .or_insert(vec![server.name()]);
            }
        }
        for (path, server_names) in server_names_by_path {
            for (idx, server_name) in server_names.iter().enumerate() {
                let is_hidden_local = idx >= 1 && target.shared;
                let local_path = local_client.real_path(&server_name, target, &path)?;
                tasks.push(PushTask {
                    local: PushLocalTask {
                        path: local_path,
                        state: PushLocalTaskState::Progress,
                        is_hidden: is_hidden_local,
                    },
                    remote: PushRemoteTask {
                        relative_path: path.to_owned(),
                        server_name: server_name.to_owned(),
                    },
                    target: target.to_owned(),
                })
            }
        }
    }

    let local_prefix_len_max = tasks
        .iter()
        .map(|target| {
            let prefix = convert_to_string(&target.local.path)?;
            Ok(prefix.len())
        })
        .collect::<Result<Vec<usize>>>()?
        .into_iter()
        .max()
        .unwrap_or(0);

    let remote_prefix_len_max = tasks
        .iter()
        .map(|target| {
            let remote_path = &remote_client.real_path(
                &target.remote.server_name,
                &target.target,
                &target.remote.relative_path,
            )?;

            let prefix = format!(
                "└─> {}@{}:{}",
                &config.remote.user,
                &target.remote.server_name,
                convert_to_string(remote_path)?
            );
            Ok(prefix.len() + 4)
        })
        .collect::<Result<Vec<usize>>>()?
        .into_iter()
        .max()
        .unwrap_or(0);

    let mut ctx = PushContext {
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
            stream.push(execute_push_task(task, &ctx));
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
        "  Finished push 🚀 [{}.{}s] ",
        elapsed.as_secs(),
        elapsed.subsec_millis()
    );

    Ok(())
}
