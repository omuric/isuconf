use anyhow::Result;
use clap::Parser;
use isuconf::commands::pull::{PullOpt, pull};
use isuconf::commands::push::{PushOpt, push};
use isuconf::commands::ssh::{SshOpt, ssh};
use isuconf::commands::ssh_config::{SshConfigOpt, ssh_config};

#[derive(Parser, Debug)]
#[command(name = "isuconf", version)]
enum Opt {
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
    let opt: Opt = Opt::parse();
    match opt {
        Opt::Pull(opt) => pull(opt).await,
        Opt::Push(opt) => push(opt).await,
        Opt::Ssh(opt) => ssh(opt).await,
        Opt::SshConfig(opt) => ssh_config(opt).await,
    }
}
