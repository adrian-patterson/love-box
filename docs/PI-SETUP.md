# Raspberry Pi provisioning runbook

How to take a blank microSD card to a running frame. It is written so an agent with SSH access can
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

`user-data` (`#cloud-config`) mirrors what Imager itself generates
(`src/customization_generator.cpp` in raspberrypi/rpi-imager): `hostname: "lovebox"`,
`manage_etc_hosts: true`, `packages: [avahi-daemon]`, `timezone: "<your tz, e.g. Europe/London>"`,
singular `user:` (not `users:`, which drops the default user's `gpio`/`spi`/`sudo` groups)
named `pi` with `lock_passwd: false`, a SHA-512 `passwd` hash (`openssl passwd -6`; the
macOS system `openssl` lacks `-6`, use Homebrew's), `ssh_authorized_keys` =
`~/.ssh/id_ed25519.pub` and `sudo: ALL=(ALL) NOPASSWD:ALL`; `ssh_pwauth: false`;
`rpi: { interfaces: { spi: true } }` so SPI is enabled at first boot; and
`runcmd: [[systemctl, enable, --now, ssh]]`, because Raspberry Pi OS ships sshd disabled and
cloud-init has no `enable_ssh` key (it is silently ignored).
`network-config`: netplan `version: 2`, `wifis.wlan0` with `dhcp4: true`,
`optional: true`, `regulatory-domain: "<your ISO 3166 country code>"`, and your Wi-Fi
SSID/password under `access-points`. The Zero W is 2.4 GHz only.

Both files hold secrets (Wi-Fi password, password hash) and live in the session scratchpad,
never in the repo. The device node comes from `diskutil list` (the Mac's built-in SDXC reader
reports the card as `internal, physical`, not external) and is confirmed by a human before
the write; the built-in reader needs a microSD-to-SD adapter. Sources:
https://www.raspberrypi.com/news/cloud-init-on-raspberry-pi-os/ and cloud-init's
`cc_raspberry_pi` module docs.

First boot takes about 13 minutes on a Zero W: `lovebox.local` resolves once cloud-init
has installed avahi, and SSH answers only after its final stage runs the `runcmd`. If the
name is not up yet, find the Pi by MAC prefix `b8:27:eb` (ping-sweep the subnet, then `arp -an`).

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

Applied 2026-10-26 as drop-ins, so package upgrades never prompt over or replace them:

- SSH: key-only. `/etc/ssh/sshd_config.d/lovebox.conf`: `PasswordAuthentication no`,
  `KbdInteractiveAuthentication no`; check with `sshd -t`, then `systemctl reload ssh`.
- `apt-get install -y unattended-upgrades` and `/etc/apt/apt.conf.d/20auto-upgrades` with
  `APT::Periodic::Update-Package-Lists "1";` and `APT::Periodic::Unattended-Upgrade "1";`
  (the non-interactive equivalent of `dpkg-reconfigure -plow unattended-upgrades`).
- Hardware watchdog: `dtparam=watchdog=on` under `[all]` in `/boot/firmware/config.txt`, and
  `RuntimeWatchdogSec=15` in `/etc/systemd/system.conf.d/lovebox-watchdog.conf`, so a hung
  kernel reboots itself. Needs a reboot; check with `systemctl show -p RuntimeWatchdogUSec`.
- Keep the SD card alive: `/etc/systemd/journald.conf.d/lovebox.conf` sets
  `SystemMaxUse=50M` under `[Journal]`; `log2ram` is optional and not installed.

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
