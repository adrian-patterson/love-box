# AGENTS.md — love-box

Read this first. It describes the repository as it is at the commit you are on. If
something here disagrees with the code, the code wins; fix this file in the same change.

## What this project is

An 800x480 black-and-white e-paper picture frame (Waveshare 7.5" HAT, V2 panel) driven by
a Raspberry Pi Zero W (v1, ARMv6, 512 MB). A phone or laptop on the home network sends it a
text message or picks a photo, and the panel updates. It is a gift, not a product: one
frame, one household, no cloud account.

Status: **rewrite in progress.** The 2022 Python/React/Firebase implementation was removed
from the tree in October 2026 (git tag `legacy-2022` holds the last commit that has it). What
exists now: a Rust crate that cross-builds for the Pi, a deploy kit, and the start of the
renderer (frame type, `Panel` trait with a PNG implementation, bundled fonts, message and
quote cards). Work is paused; [docs/PLAN.md](docs/PLAN.md) "Where things stand" says
what is done, parked, and next.
The plan is in [docs/PLAN.md](docs/PLAN.md); durable decisions are in
[docs/decisions/](docs/decisions/). Work on the next unfinished milestone there.

## Layout

| Path | What it is |
|---|---|
| `lovebox/` | The Rust crate: library (`frame`, `panel`, `fonts`, `render`) plus binary `lovebox`. `serve` logs a startup line and heartbeats, exits 0 on SIGTERM. Later: axum API, panel driver, scheduler, store. |
| `lovebox/fonts/` | Bundled SIL OFL faces (EB Garamond, Libre Baskerville, Lora, Atkinson Hyperlegible), each with its `OFL.txt`; embedded with `include_bytes!`. |
| `lovebox/tests/` | Integration tests; `golden.rs` compares rendered frames to `tests/golden/*.png` (regenerate with `UPDATE_GOLDENS=1`). |
| `lovebox/.cargo/config.toml` | Pi-target rustflags only (`target-cpu=arm1176jzf-s`). Nothing global. |
| `deploy/` | `lovebox.service` (systemd), `provision.sh` (runs on the Pi, idempotent), `deploy.sh` (build, verify ARMv6, ship, restart, rollback), `backup.sh`, `check-pi.sh`, `README.md`. |
| `Makefile` | Developer entry points; see Commands. |
| `rust-toolchain.toml` | Stable toolchain, `arm-unknown-linux-gnueabihf` target. |
| `docs/` | `PLAN.md` (spec and milestones), `UI-DESIGN.md` (web UI design contract), `PI-SETUP.md` (blank card to running Pi), `RESEARCH.md` (sourced findings), `decisions/` (ADRs). |

## Target architecture (summary; details in docs/PLAN.md)

One Rust binary, `lovebox`, on the Pi:

- `axum` HTTP server on the LAN (`http://lovebox.local`), serving an embedded web UI
  (Vite + React + TypeScript, built on the laptop) and a JSON API.
- Local state only: one JSON state document and a photo directory under `/var/lib/lovebox`
  (docs/decisions/ADR-002).
- Display module behind a `Panel` trait: `WavesharePanel` (hardware via `epd-waveshare`
  `epd7in5_v2` + `rppal`, Cargo feature `hardware`) and `PngPanel` (writes the frame to a
  PNG so everything develops and tests on a laptop with no hardware).
- Renderer produces the exact 800x480 1-bit frame for text, photos (dithered), and quotes;
  the same renderer backs the UI's preview, so preview equals panel.
- Content is a queue of cards (text, photo, quote, generated note) with a mode: pinned,
  rotate, or daily. Sources: direct text, uploaded photos, curated quotes, optional Google
  Photos Picker import, optional Immich, optional Claude-generated notes and captions.
- Operations: systemd unit, journald logs, `/healthz`, a status page, one-command deploy.

## Baseline the rewrite must preserve (from the 2022 version)

- Panel is 800x480, 1-bit. Full refresh on every change (about 4 s); deep sleep between
  updates; clear on wake to limit ghosting. Partial refresh is out of scope (the Rust driver
  has none for this panel).
- Text sizes and wrap widths: 30 pt → 58 chars/line, 15 lines; 48 pt → 35/9; 64 pt → 26/7;
  96 pt → 16/5. Overflow truncates with `...`. Text starts at x=10, y=0. The 2022 font was
  Palatino (not open-licence); pick an SIL OFL serif (for example EB Garamond or Libre
  Baskerville) and bundle it with `include_bytes!`.
- A message of exactly `clear` blanks the panel.
- Photos are cover-cropped to 800x480. The old version did no dithering; the new one must.
- Pin mapping (BCM): RST 17, DC 25, CS 8 (SPI0 CE0), BUSY 24. SPI bus 0, device 0, 4 MHz.
  The V2 panel only updates BUSY after a status command (0x71); `epd-waveshare` on `main`
  handles that (see docs/RESEARCH.md §3) but has no timeout in its busy loop, so wrap panel
  calls with our own timeout on a dedicated blocking thread.

## Commands

Run from the repo root unless noted. `rustup` lives in `~/.cargo/bin`; the Makefile adds it
to PATH itself.

| Command | What it does |
|---|---|
| `make test` | `cargo test` in `lovebox/` (native) |
| `make lint` | `cargo fmt --check && cargo clippy --all-targets -- -D warnings` |
| `make run` | `cargo run -- serve` natively |
| `make build-pi` | `cargo zigbuild --release --target arm-unknown-linux-gnueabihf.2.36` → `lovebox/target/arm-unknown-linux-gnueabihf/release/lovebox` |
| `make check-pi` | build-pi, then `deploy/check-pi.sh`: proves `Tag_CPU_arch: v6` with `readelf -A` and runs the binary under Docker emulation (see deploy/README.md for the Docker/binfmt setup) |
| `make deploy [HOST=pi@lovebox.local]` | build, static ARMv6 check, ship, restart, verify active |
| `make rollback` / `make backup` | swap `lovebox.prev` back / rsync `/var/lib/lovebox` to `~/lovebox-backup` |

Plain `cargo build --target arm-unknown-linux-gnueabihf` fails on macOS (no linker); always
go through `make build-pi`. Never target `armv7-*`: it segfaults on the Zero W.

## Working in this repo

- **Every unit that adds Rust code adds `cargo test` coverage**, runnable natively with the
  PNG panel. `make lint` must be clean before a unit is reported done.
- **Do not add cloud dependencies.** The frame must work with the house internet down.
  Google Photos, Immich, and the Claude API are optional, behind config flags, and the
  service must start and run without their keys.
- **Hardware code runs only on the Pi.** Keep SPI/GPIO behind the `Panel` trait and the
  `hardware` Cargo feature so the crate builds and tests on macOS and Linux without it.
  Add `--features hardware` to the Makefile's `build-pi` line when that feature exists.
- **No C dependencies without re-running `make check-pi`.** C code compiles through
  `zig cc`; the check catches an accidental ARMv7 object.
- **Secrets never go in git.** OAuth client secrets, refresh tokens, and API keys live in
  `/etc/lovebox/secrets.env` on the Pi (and in `.env.local` on dev machines, gitignored).
  Nothing in docs or code may contain a real SSID, password, key, or a person's name.
- **Coordination between agent sessions**, when a project manager uses one, happens on an
  ephemeral board at `.tmp/agents-messaging-board/` (gitignored). Decisions that must
  outlive a checkout are copied to `docs/decisions/`.
- Commit with a `Co-Authored-By: Claude <model> <noreply@anthropic.com>` trailer naming the
  model that wrote the change, when Claude wrote it.

## Pointers

- Plan and milestones: [docs/PLAN.md](docs/PLAN.md)
- Web UI design (screens, tokens, states, API needs): [docs/UI-DESIGN.md](docs/UI-DESIGN.md)
- Research with sources (Google Photos API status, Rust driver, toolchain): [docs/RESEARCH.md](docs/RESEARCH.md)
- Pi provisioning runbook: [docs/PI-SETUP.md](docs/PI-SETUP.md)
- Decisions: [docs/decisions/](docs/decisions/)
- Deploy kit details: [deploy/README.md](deploy/README.md)
- Rust driver: https://github.com/rust-embedded-community/epd-waveshare (module `epd7in5_v2`)
- Panel protocol reference (vendor C/Python examples): https://github.com/waveshare/e-Paper
