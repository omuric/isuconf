
use crate::config::{read_config};
use anyhow::Result;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct SshConfigOpt {
    // Config path
    #[structopt(short, long, default_value = "./isuconf.yaml")]
    pub config: String,
}

pub async fn ssh_config(opt: SshConfigOpt) -> Result<()> {
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
