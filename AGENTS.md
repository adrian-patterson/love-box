# AGENTS.md — love-box

Read this first. It describes the repository as it is at the commit you are on, and
points to the plan for where it is going. If something here disagrees with the code,
the code wins; fix this file in the same change.

## What this project is

An 800x480 black-and-white e-paper picture frame (Waveshare 7.5" HAT, V2 panel) driven by
a Raspberry Pi Zero W. A phone or laptop on the home network sends it a text message or
picks a photo, and the panel updates. It is a gift, not a product: one frame, one household.

Status: **mid-rewrite.** The code in `Api/`, `LoveBoxUi/`, and `RpiUpdateService/` is the
2022 Python + React + Firebase implementation. It is being replaced by a single Rust
binary that runs on the Pi with no cloud dependency. The new code so far: `lovebox/` (the
Rust crate, M1 skeleton), `deploy/` (systemd unit, provision/deploy/backup/check scripts),
`Makefile` and `rust-toolchain.toml` at the root. The plan is in
[docs/PLAN.md](docs/PLAN.md); durable decisions are in [docs/decisions/](docs/decisions/).
Until a milestone in the plan says otherwise, the legacy code is reference material, not
something to extend.

## Legacy layout (what is checked in today)

| Path | What it is | How it runs |
|---|---|---|
| `Api/` | Flask API, 3 routes: `POST /message`, `POST /image`, `POST /preview`. Writes one document to Firebase Realtime Database. | `python api.py` on port 8000. Needs `FIREBASE_PRIVATE_KEY` env var pointing at a service-account JSON. `docker-compose up` also works. |
| `LoveBoxUi/` | Create React App (React 18, MUI 5, TypeScript). One page: font-size select, message box, preview, photo gallery. | `npm start` (port 3000), `npm run build`, `npm test`. API host is hardcoded in `LoveBoxUi/src/config.ts`. |
| `RpiUpdateService/` | Runs on the Pi. `update_display.py` reads Firebase, and if `modified` is within the last 10 minutes, renders text or a BMP to the panel. Cron every 10 minutes. | `python3 update_display.py` from `RpiUpdateService/src`. |
| `RpiUpdateService/src/lib/` | Vendored 2019 Waveshare driver (`epd7in5_V2.py`, `epdconfig.py`) using `RPi.GPIO` + `spidev`. | Imported by `update_display.py`. |
| `RpiUpdateService/src/pic/`, `font/` | Pre-resized 800x480 BMPs and `Palatino.ttf`. | Read by `update_display.py`. |

Data model shared by all three legacy parts (one JSON object at the database root):

```json
{ "message": "text or image filename", "fontsize": 48, "image": false, "modified": "<ISO-8601 UTC>" }
```

Rendering rules the legacy code encodes and the rewrite must preserve as a baseline:

- Panel is 800x480, 1-bit. Text is drawn at x=10 from y=0 in Palatino.
- Font sizes and wrap widths: 30pt → 58 chars/line, 15 lines; 48pt → 35/9; 64pt → 26/7; 96pt → 16/5. Overflow is truncated with `...`.
- The literal message `clear` blanks the panel.
- Photos are stored already cropped to 800x480 (`format_images.py` uses `ImageOps.fit`), no dithering.

Pin mapping (BCM) from `epdconfig.py`: RST 17, DC 25, CS 8, BUSY 24. SPI bus 0 device 0 at 4 MHz.

## Known defects in the legacy code (do not "fix" unless a unit asks)

These are verified against the files named; they matter because the rewrite must not copy them.

- `RpiUpdateService/ShellScripts/auto_update.sh` and `update_display.sh` `cd` into `/home/pi/projects/the-love-box` and pull branch `main`. The repo is `love-box` and the only branch is `master`. The README's crontab lines use `love-box`.
- `RpiUpdateService/ShellScripts/install.sh` runs `pip3 install RPi.GPIO -y`; pip has no `-y` flag. It also installs `wiringpi` and `bcm2835`, which nothing in `src/` imports.
- `RpiUpdateService/src/format_images.py` uses `Image.ANTIALIAS`, removed in Pillow 10.
- `LoveBoxUi/Dockerfile` globally installs `react-scripts@3.4.1` while `package.json` pins `5.0.1`.
- `LoveBoxUi/src/images/photos.ts` lists ~130 photos; only three `.bmp` files are committed under `LoveBoxUi/public/images/` and `RpiUpdateService/src/pic/`. The rest lived only on the author's machines.
- `RpiUpdateService/src/update_display.py` uses `except KeyError or TypeError`, which only catches `KeyError`.
- `RPi.GPIO` no longer works on Raspberry Pi OS Bookworm and later; the vendored driver will not run on a fresh Pi image.

## Target architecture (summary; details in docs/PLAN.md)

One Rust binary, `lovebox`, on the Pi:

- `axum` HTTP server on the LAN (`http://lovebox.local`), serving an embedded web UI and a JSON API.
- Local state only: SQLite for settings/history, a photo directory on the SD card. No Firebase.
- Display module behind a `Panel` trait: `WavesharePanel` (real hardware via `epd-waveshare` + `rppal`) and `PngPanel` (writes the frame to a PNG so everything can be developed and tested on a Mac with no hardware).
- Renderer produces the exact 800x480 1-bit frame for text, photos (with dithering), and quotes; the same renderer backs the UI's preview, so preview equals panel.
- Content sources: direct text, local photo library (uploaded from the phone), curated quotes, optional Google Photos Picker import, optional Immich album, optional Claude-generated daily notes/captions.
- Operations: `systemd` unit, journald logs, `/healthz`, a status page, a one-command deploy from the Mac.

## Commands (new stack)

Run from the repo root unless noted. `rustup` lives in `~/.cargo/bin`; the Makefile adds it
to PATH itself.

| Command | What it does |
|---|---|
| `make test` | `cargo test` in `lovebox/` (native, macOS) |
| `make lint` | `cargo fmt --check && cargo clippy --all-targets -- -D warnings` |
| `make run` | `cargo run -- serve` natively |
| `make build-pi` | `cargo zigbuild --release --target arm-unknown-linux-gnueabihf.2.36` → `lovebox/target/arm-unknown-linux-gnueabihf/release/lovebox` |
| `make check-pi` | build-pi, then `deploy/check-pi.sh`: proves `Tag_CPU_arch: v6` and runs the binary under Docker emulation (needs `colima start` + arm binfmt; see deploy/README.md) |
| `make deploy [HOST=pi@lovebox.local]` | build, static ARMv6 check, ship, restart, verify active |
| `make rollback` / `make backup` | swap `lovebox.prev` back / rsync `/var/lib/lovebox` to `~/lovebox-backup` |

Plain `cargo build --target arm-unknown-linux-gnueabihf` fails on macOS (no linker); always
go through `make build-pi`. Never target `armv7-*`: it segfaults on the Zero W.

## Working in this repo

- **The legacy code has no tests; the Rust crate does.** Every unit that adds Rust code also adds `cargo test` coverage, runnable on macOS with the PNG panel. `make lint` must be clean.
- **Do not add cloud dependencies.** Firebase is being removed; anything new must work with the Pi unplugged from the internet, except explicitly optional integrations (Google Photos, Claude API).
- **Hardware code runs only on the Pi.** Keep SPI/GPIO behind the `Panel` trait and a Cargo feature so the crate builds and tests on macOS.
- **Secrets never go in git.** Google OAuth client secrets, refresh tokens, and Anthropic API keys live in `/etc/lovebox/` on the Pi (and in `.env.local` on dev machines, which is gitignored).
- **Coordination between agent sessions** happens on the board at `.tmp/agents-messaging-board/` (gitignored, ephemeral). Decisions that must outlive a checkout are copied to `docs/decisions/`.
- Commit with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` when Claude wrote the change.

## Pointers

- Plan and milestones: [docs/PLAN.md](docs/PLAN.md)
- Research with sources (Google Photos API status, Rust driver, toolchain): [docs/RESEARCH.md](docs/RESEARCH.md)
- Pi provisioning runbook: [docs/PI-SETUP.md](docs/PI-SETUP.md)
- Decisions: [docs/decisions/](docs/decisions/)
- Upstream driver the Python code was copied from: https://github.com/waveshare/e-Paper
- Rust driver: https://github.com/rust-embedded-community/epd-waveshare (module `epd7in5_v2`)
