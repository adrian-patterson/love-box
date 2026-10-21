# Raspberry Pi provisioning runbook

This is the procedure milestone M1 executes. It is written so an agent with SSH access can
run it step by step and so a human can audit what was done. Commands that change the Pi
are marked; everything else is read-only.

## 0. Flash the card (on the Mac)

The board is the original **Pi Zero W (v1, ARMv6)**, decided 2026-10-21. Use **32-bit
Raspberry Pi OS Lite** (Trixie, `init_format: cloudinit-rpi`); the Zero W cannot boot a
64-bit image. The image is read from the official OS list
(`https://downloads.raspberrypi.com/os_list_imagingutility_v4.json`) and verified against
its published `.sha256`.

Raspberry Pi Imager 2.0.12 is installed on the Mac and has a CLI that takes cloud-init
files directly, which is how the card is customised (no GUI clicking):

```bash
sudo "/Applications/Raspberry Pi Imager.app/Contents/MacOS/rpi-imager" --cli \
  --cloudinit-userdata user-data --cloudinit-networkconfig network-config \
  2026-10-06-raspios-trixie-armhf-lite.img.xz /dev/diskN
```

`user-data` (`#cloud-config`): `hostname: lovebox`, `manage_etc_hosts: true`,
`timezone: <your tz, e.g. Europe/London>`, `enable_ssh: true`, `ssh_pwauth: false`, user `pi` in the
standard Pi groups (including `gpio`, `spi`) with `ssh_authorized_keys` =
`~/.ssh/id_ed25519.pub`, a hashed console password, `sudo: ALL=(ALL) NOPASSWD:ALL`, and
`rpi: { spi: true }` so SPI is enabled at first boot (no `raspi-config` step needed).
`network-config`: netplan v2, `renderer: NetworkManager`, `wlan0` with `dhcp4`,
`optional: true`, `regulatory-domain: "<your ISO 3166 country code>"`, and your Wi-Fi
SSID/password under `access-points`. The Zero W is 2.4 GHz only.

Both files hold secrets (Wi-Fi password, password hash) and live in the session scratchpad,
never in the repo. The device node comes from `diskutil list external physical` and is
confirmed by a human before the write; the Mac's built-in SD reader needs a microSD-to-SD
adapter. Source for the format: https://www.raspberrypi.com/news/cloud-init-on-raspberry-pi-os/

## 1. Reach it (read-only)

```bash
ssh pi@lovebox.local 'uname -a; cat /etc/os-release | head -3; free -m; df -h /; cat /proc/cpuinfo | grep -E "Model|model name" | head -2'
```

Expected: `Raspberry Pi Zero W` in Model (if it says `Zero 2 W`, stop and report), Trixie (13).
Also: `ls /dev/spidev0.*` shows `spidev0.0` (cloud-init enabled SPI) and `hostname` is `lovebox`.

## 2. Base configuration (changes the Pi)

Hostname and SPI were set by cloud-init at first boot. If either is missing, fall back to
`sudo raspi-config nonint do_spi 0` / `do_hostname lovebox` and reboot.

```bash
sudo apt update && sudo apt full-upgrade -y
sudo apt install -y avahi-daemon rsync    # avahi gives lovebox.local; rsync is for backup.sh
sudo reboot
```

Verify after reboot: `ping lovebox.local` works from the Mac.

## 3. Hardening and maintenance (changes the Pi)

- SSH: key-only. In `/etc/ssh/sshd_config.d/lovebox.conf`: `PasswordAuthentication no`.
- `sudo apt install -y unattended-upgrades && sudo dpkg-reconfigure -plow unattended-upgrades`.
- Hardware watchdog: `dtparam=watchdog=on` in `/boot/firmware/config.txt`, and
  `RuntimeWatchdogSec=15` in `/etc/systemd/system.conf`, so a hung kernel reboots itself.
- Keep the SD card alive: `sudo apt install -y log2ram` is optional; journald is capped by
  `SystemMaxUse=50M` in `/etc/systemd/journald.conf`.

## 4. Service layout on the Pi

```
/opt/lovebox/lovebox            the binary (replaced by deploy)
/etc/lovebox/config.toml        non-secret config
/etc/lovebox/secrets.env        0600, root:lovebox; optional API keys
/var/lib/lovebox/               SQLite db, photo library, cached frames
/etc/systemd/system/lovebox.service
```

The unit runs as a dedicated `lovebox` user in groups `spi` and `gpio`, with
`Restart=on-failure`, `ProtectSystem=strict`, `ReadWritePaths=/var/lib/lovebox`.
Port 80 needs `AmbientCapabilities=CAP_NET_BIND_SERVICE`, or run on 8080 and tell people
to type the port. The unit file lives in `deploy/lovebox.service` in this repo.

Ownership, created by `deploy/provision.sh` (idempotent; run as
`sudo bash provision.sh lovebox.service` after copying both files from `deploy/`):
`/opt/lovebox` root:root 0755; `/etc/lovebox` root:lovebox 0750; `config.toml` root:lovebox
0640 (created empty if absent); `/var/lib/lovebox` and `photos/` lovebox:lovebox 0750.
`lovebox` is a system user with no login shell. provision.sh does not enable SPI, set the
hostname, reboot, or start the service.

## 5. Deploy from the Mac

`deploy/deploy.sh [HOST]` (`make deploy`): cross-compiles with `make build-pi`, refuses a
non-ARMv6 binary (`deploy/check-pi.sh --static-only`), `scp`s it to the SSH user's home,
`sudo install -o root -g root -m 0755` it as `/opt/lovebox/lovebox.new`, hard-links the
current binary to `lovebox.prev`, renames `lovebox.new` over `lovebox` (atomic), restarts
the unit, prints the last 20 lines of `journalctl -u lovebox`, and exits non-zero unless the
unit is active after 5 s. One command, idempotent. Rollback is `deploy/deploy.sh --rollback`
(`make rollback`): swaps `lovebox.prev` and `lovebox` and restarts. Needs key-based SSH and
passwordless sudo for the SSH user. Docker is not needed to deploy.

## 6. Backup

Photos and the database live under `/var/lib/lovebox`. `deploy/backup.sh` rsyncs that
directory to `~/lovebox-backup/` on the Mac. Run it before any re-image. No sudo needed if
the SSH user is in group `lovebox`: `sudo usermod -aG lovebox pi` once. DEST is an exact
mirror (`rsync --delete`). Needs `rsync` on the Pi (installed in §2).
