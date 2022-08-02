use anyhow::Result;
use isuconf::commands::list::{list, ListOpt};
use isuconf::commands::pull::{pull, PullOpt};
use isuconf::commands::push::{push, PushOpt};
use isuconf::commands::ssh::{ssh, SshOpt};
use isuconf::commands::ssh_config::{ssh_config, SshConfigOpt};
use structopt::StructOpt;

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
