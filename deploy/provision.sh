#!/usr/bin/env bash
# deploy/provision.sh UNIT_FILE
#
# UNTESTED as of U-101; first run is U-103.
#
# Runs ON the Pi, as root, and is safe to re-run. Creates the service account and the
# directory layout from docs/PI-SETUP.md §4, installs the systemd unit, and enables it.
# It does NOT enable SPI, change the hostname, or reboot: those are docs/PI-SETUP.md §2,
# done by hand. It does not start the service either; deploy.sh does that once a binary
# is in /opt/lovebox.
#
# From the Mac:
#   scp deploy/provision.sh deploy/lovebox.service pi@lovebox.local:
#   ssh pi@lovebox.local sudo bash provision.sh lovebox.service
#
# Resulting layout:
#   /opt/lovebox/                root:root      0755  binary, replaced by deploy.sh
#   /etc/lovebox/                root:lovebox   0750
#   /etc/lovebox/config.toml     root:lovebox   0640  Pi defaults if absent, never overwritten
#   /etc/lovebox/secrets.env     (not created)  0600 root:lovebox when you add it by hand
#   /var/lib/lovebox/            lovebox:lovebox 0750  state; group-readable for backup.sh
#   /var/lib/lovebox/photos/     lovebox:lovebox 0750
#   /etc/systemd/system/lovebox.service  root:root 0644
set -euo pipefail

unit_src="${1:?usage: sudo bash provision.sh path/to/lovebox.service}"
[[ $EUID -eq 0 ]] || { echo "provision: must run as root (sudo bash $0 $unit_src)" >&2; exit 1; }
[[ -f "$unit_src" ]] || { echo "provision: no such unit file: $unit_src" >&2; exit 1; }

# spi and gpio are created by Raspberry Pi OS; their absence means this is not one.
for group in spi gpio; do
  getent group "$group" > /dev/null \
    || { echo "provision: group '$group' missing; is this Raspberry Pi OS?" >&2; exit 1; }
done

# Service account: system user, own group, no login shell, no home directory.
if id -u lovebox > /dev/null 2>&1; then
  usermod --append --groups spi,gpio lovebox
  echo "provision: user lovebox exists; ensured groups spi,gpio"
else
  useradd --system --user-group --groups spi,gpio \
    --home-dir /var/lib/lovebox --no-create-home --shell /usr/sbin/nologin lovebox
  echo "provision: created system user lovebox (groups spi,gpio)"
fi

install -d -o root -g root -m 0755 /opt/lovebox
install -d -o root -g lovebox -m 0750 /etc/lovebox
install -d -o lovebox -g lovebox -m 0750 /var/lib/lovebox
install -d -o lovebox -g lovebox -m 0750 /var/lib/lovebox/photos
echo "provision: directories /opt/lovebox /etc/lovebox /var/lib/lovebox/photos in place"

if [[ ! -e /etc/lovebox/config.toml ]]; then
  tmp="$(mktemp)"
  printf '%s\n' \
    '# lovebox configuration (non-secret). Values here are overridden by LOVEBOX_* env vars' \
    '# and then by command-line flags.' \
    '# Secrets (API keys, tokens) go in /etc/lovebox/secrets.env, mode 0600 root:lovebox.' \
    'bind = "0.0.0.0:80"' \
    'data_dir = "/var/lib/lovebox"' \
    'panel = "waveshare"' \
    'decode_limit_mb = 160' \
    'log = "info"' \
    > "$tmp"
  install -o root -g lovebox -m 0640 "$tmp" /etc/lovebox/config.toml
  rm -f "$tmp"
  echo "provision: wrote /etc/lovebox/config.toml"
else
  echo "provision: /etc/lovebox/config.toml exists; left unchanged"
fi

install -o root -g root -m 0644 "$unit_src" /etc/systemd/system/lovebox.service
systemctl daemon-reload
systemctl enable lovebox
echo "provision: unit installed and enabled (not started; run deploy.sh from the Mac)"
