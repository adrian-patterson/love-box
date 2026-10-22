# love-box

An e-paper picture frame you control from your phone. Send a note, pick a photo, or let it
rotate through your favourites; the panel updates in seconds. Built as a gift for leaving
messages for someone you love. Runs entirely on a Raspberry Pi Zero W on your home network,
with no cloud account.

## The frame

### Displaying a picture
![image](https://user-images.githubusercontent.com/47571939/163594413-c6eb3808-4513-4128-835d-d810d2fb4636.png)
### Displaying text
![image](https://user-images.githubusercontent.com/47571939/163594460-b78e4632-db6d-4be3-b008-e5df3094526b.png)
### The wiring
![image](https://user-images.githubusercontent.com/47571939/163594511-85c280f8-d6a0-4ccc-a2cf-58c4714fc34f.png)
![image](https://user-images.githubusercontent.com/47571939/163594503-563f6b02-9094-4581-a29f-ac31b20f1bf2.png)
### The case for the Pi and the Waveshare HAT
![image](https://user-images.githubusercontent.com/47571939/163594571-d0f96c77-cdce-4f6a-8a29-8d045d3c6322.png)
![image](https://user-images.githubusercontent.com/47571939/163594608-162ef34d-f450-4de3-981d-98be82ceefe8.png)
![image](https://user-images.githubusercontent.com/47571939/163594590-a1441343-8cd4-471c-af65-96cdf93154f1.png)

## What it does

- **Web page on your LAN** at `http://lovebox.local`. Anyone on the Wi-Fi can send a message
  or choose a photo from a phone. No app, no account, no internet needed.
- **Sources**: a typed message in four sizes, a photo library you upload to from your phone,
  photos picked from Google Photos (through Google's Picker API; automatic album-following
  is no longer possible for third-party apps), curated quotes, and optional AI-written daily
  notes and photo captions through the Claude API.
- **Slideshow**: cycle through your photos, shuffled or in order, at an interval you set in
  the UI. Default 15 minutes; the panel maker recommends at least 3 minutes between
  refreshes, and the UI lets you go down to 15 seconds behind a warning. Every change is a
  full refresh: about 4 seconds, with a visible black-and-white flash, and faster cycling
  wears the panel sooner. Quiet hours stop refreshes at night.
- **Other modes**: pin one message or photo until you change it, or a fresh card each
  morning.
- **Preview equals panel**: the page shows the exact 800x480 frame the e-paper will show.
- **Boring to operate**: one-command deploy from your laptop, a systemd service, a health
  page, backups, and everything testable on a laptop without the hardware.

Status (October 2026): the renderer is accepted: messages, auto-fitted quotes up to 2,000 characters, memory-budgeted photos, dithering, PNG output and development CLI. The toolchain, ARMv6 cross-build and deploy kit exist. The local server/API, atomic state store, photo library and laptop import are accepted. The embedded local web UI is accepted. The compile-only panel driver is next; deployment and on-panel validation wait for hardware. Progress: [docs/PLAN.md](docs/PLAN.md).

## Hardware

| Part | Notes |
|---|---|
| [Waveshare 7.5" e-Paper HAT](https://www.waveshare.com/7.5inch-e-paper-hat.htm), V2 panel | 800x480, 1-bit black and white, SPI. Every update is a ~4 s full refresh with a flash; no partial refresh in the Rust driver. Waveshare advises ≥180 s between refreshes for panel life |
| Raspberry Pi Zero W | The original, single-core ARMv6, 512 MB. A Zero 2 W also fits the same case and HAT |
| 5 V / 2.5 A micro-USB power supply | Into the port marked PWR IN. A thin or long cable causes brownouts |
| microSD card, 8 GB or more | Buy one that ships with an SD adapter for flashing |
| 7"x5.5" picture frame and a 3D-printed case | Case holds the Pi with the HAT on its header |

Wiring is the HAT on the Pi's 40-pin header. Pins used (BCM numbering): RST 17, DC 25,
CS 8 (SPI0 CE0), BUSY 24; SPI0 at 4 MHz. SPI is enabled at first boot by the setup in
[docs/PI-SETUP.md](docs/PI-SETUP.md).

## Software

| Layer | Choice |
|---|---|
| OS | Raspberry Pi OS Lite, 32-bit (Trixie), headless, configured with cloud-init at first boot |
| Service | One Rust binary, `lovebox`, run by systemd as an unprivileged user |
| Web | `axum` + `tokio`, JSON API, embedded static UI (Vite + React + TypeScript) |
| Panel | `epd-waveshare` (`epd7in5_v2`) over `rppal` SPI/GPIO, behind a `Panel` trait with a PNG implementation for development |
| Rendering | `image` crate, Floyd–Steinberg dithering to 1-bit, text layout with an open-licence serif |
| State | One JSON state document plus a photo directory under `/var/lib/lovebox`; no database |
| Network | mDNS (`lovebox.local`) via avahi; LAN only |
| Build | Cross-compiled on a laptop with `cargo-zigbuild` to `arm-unknown-linux-gnueabihf`; `make build-pi` |

The build/deploy kit and renderer are implemented; the remaining layers are specified in the plan.

## Repository

```
lovebox/     Rust crate: the service
deploy/      systemd unit, provision/deploy/backup scripts, ARMv6 check
docs/        PLAN.md (design and milestones), PI-SETUP.md (blank card to running Pi),
             RESEARCH.md (sourced findings), decisions/ (ADRs)
Makefile     make test | lint | run | build-pi | check-pi | deploy | rollback | backup
AGENTS.md    entry point for AI coding agents
```

## Getting started

1. Flash a card and bring the Pi onto your Wi-Fi: [docs/PI-SETUP.md](docs/PI-SETUP.md).
2. On your laptop: install [rustup](https://rustup.rs), then `cargo install cargo-zigbuild`
   and a `zig` it can find (see [deploy/README.md](deploy/README.md)).
3. `make test`, then `make deploy HOST=pi@lovebox.local`.

### With an AI coding agent

[AGENTS.md](AGENTS.md) tells an agent what the project is, the rendering rules to preserve,
the commands that exist, and the rules for working here. Open the repo in Claude Code or any
agent that reads `AGENTS.md` and point it at the next unfinished milestone in
`docs/PLAN.md`. Secrets (Wi-Fi, OAuth client, API keys) never go in the repo.

## History

The first version (2022) used Python, Firebase, a Flask API, and a React app, and updated
the panel from a 10-minute cron job. It stopped working on current Raspberry Pi OS and was
removed in October 2026 when the Rust rewrite began. It is still in git history at tag
`legacy-2022` if you want it.
