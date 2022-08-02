
use crate::config::{read_config};
use anyhow::Result;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct SshOpt {
    // Config path
    #[structopt(short, long, default_value = "./isuconf.yaml")]
    pub config: String,
    // Server name
    #[structopt(name = "SERVER_NAME")]
    pub server_name: String,
}

pub async fn ssh(opt: SshOpt) -> Result<()> {
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
