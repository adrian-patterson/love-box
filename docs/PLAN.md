# love-box revamp plan

Status: **accepted by Adrian 2026-10-21** (board + three defaults). Once accepted, this file is the
specification of record; milestones get ticked here and decisions get recorded in
`docs/decisions/`.

## Goals

1. **Fully local.** No Firebase, no cloud account required for the core experience. The
   frame works with the house internet down.
2. **On the LAN.** Anyone on the home Wi-Fi opens `http://lovebox.local` on their phone and
   can change what the frame shows. Changes appear in seconds, not within a 10-minute cron.
3. **Rust, one binary, on the Pi.** Web server, renderer, scheduler, and panel driver in a
   single `lovebox` executable. No Python, no separate API host, no React build on a
   server somewhere else.
4. **Sources beyond a typed message:** a local photo library, Google Photos (as far as
   Google allows), curated quotes, and optional AI-generated notes.
5. **Maintainable by one person with an AI assistant:** reproducible Pi setup, one-command
   deploy, logs, health, backups, and tests that run on a Mac without hardware.

## Non-goals (for now)

- Colour or partial refresh. The panel is 1-bit; the Rust driver has no partial-refresh
  support for this model. Full refresh on every change is fine for a frame.
- Multi-frame or multi-household support. One frame, one config.
- Public internet exposure. LAN only; remote access, if ever, goes through Tailscale.

## Decisions (all answered 2026-10-21)

1. **Board: DECIDED 2026-10-21 — keep the Pi Zero W (v1, ARMv6).** Rust target
   `arm-unknown-linux-gnueabihf` (Tier 2 with host tools), with
   `arm-unknown-linux-musleabihf` as the static fallback if glibc or linker issues bite.
   32-bit Raspberry Pi OS Lite. Cross-compile from the Mac; never build on the Pi.
   Image processing must stay frugal: dither at 800x480 only, decode large uploads at a
   reduced scale, one worker thread.
2. **Photo source priority: DECIDED — phone upload first (M3), Google Photos Picker import as
   a spike (M5), Immich optional.** True album-follow is not possible via Google's API since
   March 2025; see docs/RESEARCH.md §1–2.
3. **Re-image the SD card: DECIDED — yes.** Clean 32-bit Raspberry Pi OS Lite, current
   stable release. The card has not been flashed yet as of 2026-10-21; the Pi is not on Wi-Fi.
4. **AI features: DECIDED — start with the daily note and photo captions** (Ideas #1 and #2),
   each as its own unit in M6, behind config flags.

## Target architecture

```
phone / laptop ──http://lovebox.local──▶ lovebox (Rust, on the Pi)
                                           ├─ web UI (embedded static files, Vite+React+TS)
                                           ├─ JSON API (axum)
                                           ├─ state: SQLite + /var/lib/lovebox/photos/
                                           ├─ scheduler (tokio): rotation, quiet hours, daily jobs
                                           ├─ renderer: text / photo / quote → 800x480 1-bit frame
                                           │     (same code renders the UI preview as PNG)
                                           ├─ Panel trait
                                           │     ├─ WavesharePanel  (epd-waveshare + rppal, feature "hardware")
                                           │     └─ PngPanel        (writes frame.png; default on macOS)
                                           └─ optional sources: Google Photos Picker, Immich, Claude API
```

Key properties:

- **Everything runs on a Mac without hardware.** `cargo run` serves the UI and writes each
  "displayed" frame to `./out/frame.png`. Tests assert on rendered frames.
- **Preview equals panel.** The `/api/preview` endpoint returns the exact frame as PNG, so
  the UI shows what the e-paper will show, replacing the old text-wrap guess.
- **Display access is serialised** on one blocking thread with a timeout around the busy
  wait, so a wedged panel never hangs the server. Panel goes to deep sleep after each update.
- **Content is a queue of "cards."** A card is text, a photo, a quote, or a generated
  note, with a render style. The scheduler picks the next card by mode: *pinned* (show this
  until I change it, today's behaviour), *rotate* (every N hours), *daily* (new card at
  07:00). This one abstraction carries every feature below.

### Repository layout after the rewrite

```
lovebox/          Rust crate (binary). src/{api,render,panel,sources,scheduler,store}.rs
web/              Vite + React + TypeScript UI, built to lovebox/web-dist and embedded
deploy/           lovebox.service, deploy.sh, backup.sh, cross config
docs/             this plan, research, runbook, decisions/
legacy/           Api/, LoveBoxUi/, RpiUpdateService/ moved here at M3, deleted after M4
```

## Milestones

Each milestone ends with an acceptance check Adrian can do from a phone or a terminal.
One writer at a time; a reader only for named questions. Board at
`.tmp/agents-messaging-board/`.

### M0 — Context and plan accepted  ✅ 2026-10-21
Deliverables: AGENTS.md, docs/, board, decisions D-001..D-003. Accepted by Adrian in chat.

### M1 — Pi reachable, provisioned, and deployable  *(active)*
Units: U-101 Mac toolchain + `lovebox` crate skeleton + cross-build + deploy scripts —
**accepted 2026-10-21 (D-004)**; U-102 flash SD card — image and cloud-init files prepared,
**blocked on hardware**: a microSD-to-SD adapter (or new card with adapter) and a micro-USB
power supply are on order; U-103 first SSH, provision per docs/PI-SETUP.md, deploy
hello-world — blocked on U-102.
- SSH key access to the Pi; clean OS; SPI enabled; `lovebox.local` resolves.
- Rust toolchain on the Mac (`rustup`), cross target installed, `deploy/deploy.sh` ships a
  hello-world binary and runs it under systemd.
- Acceptance: `ssh lovebox.local` works; `systemctl status lovebox` is active; journal shows
  the hello binary's log line.

### M2 — Rust drives the panel
- `lovebox` crate skeleton with `Panel` trait, `PngPanel`, `WavesharePanel`.
- Renderer: text with the legacy font sizes and wrap widths, photo with Floyd–Steinberg
  dithering and cover-crop, "clear."
- A CLI subcommand `lovebox show --text "hi"` / `--photo x.jpg` updates the real panel.
- Acceptance: a photo and a 48pt message appear on the panel, no hang, panel sleeps after.
  `cargo test` passes on the Mac with golden-frame tests.

### M3 — Local web app replaces the old stack
- axum server, SQLite store, photo library with upload (multipart; HEIC/JPEG/PNG), card
  queue, pinned mode, `/api/preview`, `/healthz`.
- UI: message composer with live preview, photo grid with upload, "showing now" header,
  status page (uptime, last refresh, disk, temperature).
- Legacy directories moved to `legacy/`; README rewritten.
- Acceptance: from a phone on Wi-Fi, send a message and upload a photo; the panel updates
  within 10 seconds of tapping send.

### M4 — Scheduling and quotes
- Rotate mode, daily mode, quiet hours (no refresh at night), a curated quotes list editable
  in the UI, quote render style (typographic layout, author line).
- Acceptance: set "rotate every 2h", observe two changes; set a quote, see it rendered.

### M5 — Google Photos Picker import *(spike, go/no-go)*
- One-time OAuth on the Mac (CLI) stores a refresh token on the Pi; the UI shows
  "Import from Google Photos" which creates a Picker session and shows the link/QR; the
  Pi polls and downloads picked items into the library.
- Decision point: if refresh tokens expire weekly for an unverified app, decide between
  re-authorising occasionally, the scraper, or Immich.
- Acceptance: pick five photos on the phone, they appear in the library within a minute.

### M6 — AI features *(each its own unit; pick from Ideas)*

### M7 — Operations polish
- Backup script, watchdog, unattended upgrades, OTA self-update option (binary checks a
  GitHub release once a day), docs updated, AGENTS.md re-audited with context-fix.

## Ideas (AI and otherwise), ranked by value ÷ effort

All AI calls go from the Pi to the Claude API over HTTPS; the Pi does no inference. Each is
behind a config flag and a key in `/etc/lovebox/secrets.env`, and the frame works without it.

1. **Daily note.** At 07:00 Claude writes a 1–3 line note, poem, or quote in a voice you
   configure ("warm, a bit funny, never cheesy"), given the date, named anniversaries, and
   optionally the weather. Rendered in the quote style. Cost: fractions of a cent per day.
2. **Photo captions.** On upload, Claude (vision) writes a one-line caption; EXIF gives the
   date and place. Rendered as a small footer on the photo, and stored for #3.
3. **Ask the frame.** Type "something from the chalet trip" or "a cosy one" in the UI;
   Claude picks from the library using the stored captions. Semantic search over your own
   photos, no cloud photo service needed.
4. **On this day.** Scheduler prefers photos taken on today's date in earlier years (EXIF).
   No AI needed; pairs well with #1 which can mention it.
5. **Morning card.** Weather + today's calendar (ICS URL) + the daily note, laid out as a
   dashboard. Useful if the frame lives in the kitchen.
6. **Generated line art.** An image model draws a 1-bit-friendly illustration from the
   daily note; dithered onto the panel. Fun, but more cost and more variance in quality.
7. **Two-way notes.** A tiny "reply" box in the UI logs a response from the person who
   saw the note, shown to the sender. Keeps the gift's original purpose.

## Risks

- **Google OAuth app lifecycle** (unverified apps, token expiry). Contained in M5 as a spike.
- **ARMv6 toolchain friction** if staying on the Zero W. Mitigation: musl static target.
- **Panel driver edge cases** (busy-wait with no timeout, wake-after-sleep ghosting).
  Mitigation: our own timeout wrapper, clear-on-wake, hardware test in M2 before anything
  else is built on it.
- **SD card wear.** Mitigation: journald cap, few writes, backups.
