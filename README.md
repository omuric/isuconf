# isuconf

isuconf is tool for manage remote server configs via ssh.

![](.img/screenshot.png)

## Installing

TBW

## Configuration

examples

```yml
remote:
  servers:
    - is1
    - is2
  user: ubuntu
local:
  config_root_path: ./config
targets:
  - path: /etc/hosts
    push: true
    pull: true
    sudo: true
    only: true
  - path: /etc/sysctl.conf
    push: true
    pull: true
    sudo: true
    only: false
  - path: ~/.env
    push: true
    pull: true
    sudo: false
    only: false
```

TBW
## Usage

TBW

