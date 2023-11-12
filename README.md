# isuconf

Isuconf is tool for manage remote server configs via ssh.  
This is a tool designed for use with ISUCON.

![](.img/screenshot.png)

## Installing

Download the binary directly.

```bash
wget https://github.com/omuric/isuconf/releases/download/0.2.6/isuconf_0.2.6_x86_64-unknown-linux-musl.zip
unzip isuconf_*_x86_64-unknown-linux-musl.zip isuconf
rm isuconf_*_x86_64-unknown-linux-musl.zip
./isuconf --help
```

(Optional) Place it in `/usr/local/bin`.

```bash
sudo mv ./isuconf /usr/local/bin/isuconf
```

Or build by yourself.

```bash
git clone git@github.com:omuric/isuconf.git
cd isuconf
cargo install --path .
```

TODO: Change to installation via Crates.io

## Configuration

isuconf.yaml

```yml
remote:
  servers:
    - alias: is1
      host: xx.xx.xx.xx
    - alias: is2
      host: xx.xx.xx.xx
  user: ubuntu
  identity: ~/.ssh/isucon.pem
local:
  config_root_path: ./config
targets:
  - path: /etc/hosts
    sudo: true
  - path: /etc/sysctl.conf
    sudo: true
    shared: true

```

| property      |                 | type    | description                                                                                                                                                                                                                  | 
|---------------|-----------------|---------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------| 
| remote        | servers         | array   | Target remote servers.                                                                                                                                                                                                       | 
|               | user            | string  | User to operate remote server.                                                                                                                                                                                               | 
|               | identity        | string  | Identity file to connect remote server.  (optional)                                                                                                                                                                          | 
|               | timeout         | string  | Timeout seconds on remote server connection. (default: 5)                                                                                                                                                                    | 
| server        | alias           | string  | Remote server alias name. (optional)                                                                                                                                                                                         | 
|               | host            | string  | Remote server hostname.                                                                                                                                                                                                      | 
| local         | config_root_dir | string  | Root directory of the configuration to be placed locally.                                                                                                                                                                    | 
| targets       |                 | array   | Target configs.                                                                                                                                                                                                              | 
| target        | path            | string  | Config path. (file or directory)                                                                                                                                                                                             | 
|               | push            | boolean | Push local config. (default: true)                                                                                                                                                                                           |
|               | pull            | boolean | Pull remote config. (default: true)                                                                                                                                                                                          | 
|               | sudo            | boolean | Use sudo to operate the remote configuration. (default: false)                                                                                                                                                               | 
|               | shared          | boolean | Use the same configuration for all remote servers. (default: false)<br>The layout of the local file is as follows.<br>`false`: `./{local.config_root_dir}/{server}/{config}`<br>`true`: `./{local.config_root_dir}/{config}` | 
| concurrency   |                 | number  | Number of parallel task executions. (default: 10)                                                                                                                                                                            | 
| max_file_size |                 | string  | Maximum file size. (default: 300k)<br>Files larger than this will be skipped.                                                                                                                                                | 

## Usage

```bash
# Specify the cli configuration file. (default: ./isuconf.yaml)
isuconf pull --dry-run --config ./isuconf.yaml
# Pull configuration files from remote servers.
isuconf pull --dry-run
isuconf pull
# Push configuration files to remote servers.
isuconf push --dry-run
isuconf push
# Operate only on the specified path.
isuconf pull /etc/hosts
# Helper command for ssh
isuconf ssh is1
# Print ~/.ssh/config
isuconf ssh-config
```
