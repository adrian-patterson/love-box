#!/usr/bin/env bash
# deploy/deploy.sh [--rollback] [HOST]
#
# First run against the Pi 2026-10-26.
#
# Ships the lovebox binary to the Pi and restarts the service. HOST defaults to
# pi@lovebox.local. Re-running it is safe.
#   1. make build-pi, then deploy/check-pi.sh --static-only (refuses a non-ARMv6 binary)
#   2. scp to the SSH user's home, then `sudo install -o root -g root -m 0755` it as
#      /opt/lovebox/lovebox.new (scp alone cannot create a root-owned file in /opt)
#   3. rotate: lovebox -> lovebox.prev (hard link), lovebox.new -> lovebox (atomic rename)
#   4. systemctl restart lovebox; after 5 s print the last 20 journal lines and exit
#      non-zero unless the unit is active
# --rollback swaps lovebox.prev and lovebox, then does step 4.
#
# Needs on the Pi: provision.sh already run; the SSH user has passwordless sudo
# (Raspberry Pi OS default for the first user) and key-based SSH (BatchMode, no prompts).
set -euo pipefail

rollback=0
if [[ "${1:-}" == "--rollback" ]]; then
  rollback=1
  shift
fi
host="${1:-pi@lovebox.local}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="$repo_root/lovebox/target/arm-unknown-linux-gnueabihf/release/lovebox"
ssh_opts=(-o BatchMode=yes -o ConnectTimeout=10)

# Runs on the Pi after the binary is in place: restart, wait, show journal, check state.
restart_and_verify="$(cat << 'REMOTE'
sudo -n systemctl restart lovebox
sleep 5
sudo -n journalctl -u lovebox -n 20 --no-pager
state="$(systemctl is-active lovebox || true)"
echo "deploy: lovebox is ${state} after 5 s"
[ "$state" = active ] || { echo "deploy: FAIL: service not active; rollback: deploy.sh --rollback" >&2; exit 1; }
REMOTE
)"

preflight="$(cat << 'REMOTE'
set -euo pipefail
cd /opt/lovebox 2> /dev/null || { echo "deploy: /opt/lovebox missing; run provision.sh first" >&2; exit 1; }
systemctl cat lovebox > /dev/null 2>&1 || { echo "deploy: lovebox.service not installed; run provision.sh first" >&2; exit 1; }
sudo -n true 2> /dev/null || { echo "deploy: $(id -un) needs passwordless sudo on the Pi" >&2; exit 1; }
REMOTE
)"

install_new="$(cat << 'REMOTE'
sudo -n install -o root -g root -m 0755 "$HOME/lovebox.new" /opt/lovebox/lovebox.new
rm -f "$HOME/lovebox.new"
if [ -e /opt/lovebox/lovebox ] && cmp -s /opt/lovebox/lovebox.new /opt/lovebox/lovebox; then
  echo "deploy: binary unchanged; keeping lovebox.prev as is"
  sudo -n rm -f /opt/lovebox/lovebox.new
else
  if [ -e /opt/lovebox/lovebox ]; then
    sudo -n ln -f /opt/lovebox/lovebox /opt/lovebox/lovebox.prev
  fi
  sudo -n mv -f /opt/lovebox/lovebox.new /opt/lovebox/lovebox
  echo "deploy: installed /opt/lovebox/lovebox (previous kept as lovebox.prev)"
fi
REMOTE
)"

swap_prev="$(cat << 'REMOTE'
[ -e /opt/lovebox/lovebox.prev ] || { echo "deploy: no /opt/lovebox/lovebox.prev to roll back to" >&2; exit 1; }
sudo -n ln -f /opt/lovebox/lovebox /opt/lovebox/lovebox.swap
sudo -n mv -f /opt/lovebox/lovebox.prev /opt/lovebox/lovebox
sudo -n mv -f /opt/lovebox/lovebox.swap /opt/lovebox/lovebox.prev
echo "deploy: rolled back; the replaced binary is now lovebox.prev"
REMOTE
)"

if (( rollback )); then
  echo "deploy: rolling back on $host"
  ssh "${ssh_opts[@]}" "$host" bash -s <<< "$preflight"$'\n'"$swap_prev"$'\n'"$restart_and_verify"
  exit 0
fi

make -C "$repo_root" build-pi
"$repo_root/deploy/check-pi.sh" --static-only "$bin"

echo "deploy: shipping $(basename "$bin") to $host"
ssh "${ssh_opts[@]}" "$host" bash -s <<< "$preflight"
scp "${ssh_opts[@]}" -q "$bin" "$host:lovebox.new"
ssh "${ssh_opts[@]}" "$host" bash -s <<< "set -euo pipefail"$'\n'"$install_new"$'\n'"$restart_and_verify"
