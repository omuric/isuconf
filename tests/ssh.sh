#!/usr/bin/env bash
# Start an isolated loopback SSH daemon using disposable keys. Requires sudo and sshd.
set -euo pipefail
ssh_test_privilege=(sudo -n)
ssh_test_use_pam=yes
if [[ "${ISUCONF_TEST_NO_SUDO:-0}" == 1 ]]; then
  ssh_test_privilege=()
  ssh_test_use_pam=no
fi
ssh_test_dir=$(mktemp -d /tmp/isuconf-sshd.XXXXXX)
cleanup() {
  local exit_status=$?
  if [[ "$exit_status" != 0 && -f "$ssh_test_dir/log" ]]; then
    cat "$ssh_test_dir/log" >&2 || true
  fi
  if [[ -f "$ssh_test_dir/pid" ]]; then
    "${ssh_test_privilege[@]}" kill "$(cat "$ssh_test_dir/pid")" || true
  fi
  rm -rf "$ssh_test_dir"
  exit "$exit_status"
}
trap cleanup EXIT
ssh-keygen -q -t ed25519 -N '' -f "$ssh_test_dir/host"
ssh-keygen -q -t ed25519 -N '' -f "$ssh_test_dir/client"
# Pick an unused local port; sshd startup fails visibly if it is taken in the interim.
ssh_test_port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
cat > "$ssh_test_dir/config" <<EOF
ListenAddress 127.0.0.1
Port $ssh_test_port
HostKey $ssh_test_dir/host
PidFile $ssh_test_dir/pid
AuthorizedKeysFile $ssh_test_dir/client.pub
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
# PAM allows key login for CI accounts whose password is locked.
UsePAM $ssh_test_use_pam
AllowUsers $(id -un)
EOF
if [[ "${ISUCONF_TEST_NO_SUDO:-0}" != 1 ]]; then
  sudo -n mkdir -p /run/sshd
fi
"${ssh_test_privilege[@]}" "${SSHD_BIN:-/usr/sbin/sshd}" -f "$ssh_test_dir/config" -E "$ssh_test_dir/log"
# openssh reads the host using the URI port syntax in ISUCONF_TEST_HOST.
export ISUCONF_TEST_HOST="127.0.0.1:$ssh_test_port"
export ISUCONF_TEST_IDENTITY="$ssh_test_dir/client"
cargo test --locked --test ssh ssh_roundtrip -- --ignored --nocapture

# The address stays the same while the server presents a different host key.
"${ssh_test_privilege[@]}" kill "$(cat "$ssh_test_dir/pid")"
rm -f "$ssh_test_dir/pid" "$ssh_test_dir/host" "$ssh_test_dir/host.pub"
ssh-keygen -q -t ed25519 -N '' -f "$ssh_test_dir/host"
"${ssh_test_privilege[@]}" "${SSHD_BIN:-/usr/sbin/sshd}" -f "$ssh_test_dir/config" -E "$ssh_test_dir/log"
ISUCONF_TEST_EXPECT_HOST_KEY_CHANGE=1 cargo test --locked --test ssh rejects_changed_host_key -- --ignored --nocapture
