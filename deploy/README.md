# deploy/: build, check, ship, roll back, back up

Run everything from the repo root on the Mac. The Pi is a Zero W v1 (ARMv6, 32-bit
Raspberry Pi OS Lite); never build on it.

| Command | What it does |
|---|---|
| `make build-pi` | `cargo zigbuild --release --target arm-unknown-linux-gnueabihf.2.36` → `lovebox/target/arm-unknown-linux-gnueabihf/release/lovebox` |
| `make check-pi` | build, then `deploy/check-pi.sh`: (a) `file` says ELF 32-bit ARM EABI5, (b) `greadelf -A` says `Tag_CPU_arch: v6*` (fails on v7+), (c) runs `--version` in `arm32v7/debian:bookworm-slim` under Docker qemu. qemu runs ARMv7 code too, so (b) is the ARMv6 proof |
| `make deploy [HOST=pi@lovebox.local]` | `deploy/deploy.sh`: build, check (a)+(b), scp, `sudo install` as `/opt/lovebox/lovebox.new`, rotate to `lovebox` (old kept as `lovebox.prev`), restart, print 20 journal lines, exit 1 unless active after 5 s |
| `make rollback [HOST=...]` | `deploy/deploy.sh --rollback`: swap `lovebox.prev` and `lovebox`, restart, same check |
| `make backup [HOST=...] [DEST=~/lovebox-backup]` | `deploy/backup.sh`: `rsync -az --delete` of `/var/lib/lovebox/` into DEST (an exact mirror) |
| `make test` / `make lint` / `make run` | native `cargo test` / `fmt --check` + `clippy -D warnings` / `cargo run -- serve` |

First time on a fresh Pi (after docs/PI-SETUP.md §1–3):

```bash
scp deploy/provision.sh deploy/lovebox.service pi@lovebox.local:
ssh pi@lovebox.local sudo bash provision.sh lovebox.service   # user, dirs, unit; idempotent
ssh pi@lovebox.local sudo usermod -aG lovebox pi               # lets backup.sh read state without sudo
make deploy
```

`provision.sh` does not enable SPI, set the hostname, or reboot (PI-SETUP §2 does).
`deploy.sh` needs key-based SSH (BatchMode) and passwordless sudo for the SSH user.
Status as of U-101: scripts exercised only against a fake Pi (arm32v7 Debian container
with stub systemctl/sudo); the unit file is unverified. First real run is U-103.

## Cross-compile route: cargo-zigbuild (route 1 of 3)

Chosen because it passed `make check-pi` first: no Docker needed to build, glibc floor
pinned to Bookworm's 2.36 by the `.2.36` target suffix, cold release build ~13 s,
stripped binary ~1.3 MB. `cross` (Docker) and the musl static target were not needed.
`lovebox/.cargo/config.toml` pins `-C target-cpu=arm1176jzf-s` for this target only.
Do not use any `armv7-*` target: it crashes on a Zero W.

## Installed on the Mac (U-101, 2026-10-21)

- rustup (official installer) → stable 1.99.1 + `arm-unknown-linux-gnueabihf`, pinned by
  `rust-toolchain.toml`. It added `. "$HOME/.cargo/env"` to `~/.zshenv` and `~/.profile`.
- `cargo install --locked cargo-zigbuild` (0.24.0).
- zig 0.17.0 from PyPI `ziglang` in `~/.local/share/ziglang-venv` (Homebrew's zig could not
  be installed: its lock files belong to another macOS user). The Makefile points
  cargo-zigbuild at it via `CARGO_ZIGBUILD_PYTHON_PATH`; a `zig` on PATH also works.
- `brew install binutils shellcheck` (binutils is keg-only: `/opt/homebrew/opt/binutils/bin/greadelf`).
- Docker is colima, not Docker Desktop. `colima start` first. Its VM has no ARM emulator
  by default; after every VM restart run
  `docker run --privileged --rm tonistiigi/binfmt --install arm` (check-pi prints this hint).
