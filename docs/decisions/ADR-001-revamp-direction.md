# ADR-001: Rewrite love-box as a single local Rust service

Date: 2026-10-21 · Status: accepted 2026-10-21

## Context

The 2022 implementation needs Firebase, a separately hosted Flask API, a separately hosted
React build, and a Python driver that no longer runs on current Raspberry Pi OS. The
display only updates on a 10-minute cron. The maintainer wants it fully local, LAN-reachable,
faster, and written in Rust, with Google Photos and quotes as sources and room for AI
features.

## Decision

- One Rust binary on the Pi (`axum` + `epd-waveshare` + `rppal` + SQLite), embedded web UI.
- Hardware behind a `Panel` trait with a PNG implementation so development and tests run on macOS.
- Photo sources in priority order: phone upload → Google Photos Picker import → Immich (optional).
  No scraping of Google pages unless the maintainer explicitly accepts the fragility.
- Hardware stays the original Pi Zero W (v1, ARMv6, 512 MB); target `arm-unknown-linux-gnueabihf`,
  musl fallback. Decided by the maintainer 2026-10-21.
- Milestones M1–M7 as listed in docs/PLAN.md; one writer at a time.

## Authority order

1. docs/PLAN.md (behaviour and scope), as accepted by the maintainer.
2. docs/RESEARCH.md (technical constraints, with sources).
3. The rendering baseline recorded in AGENTS.md (font sizes, wrap widths, pin map), taken
   from the 2022 code at tag `legacy-2022`.
4. Upstream docs for epd-waveshare, rppal, axum.

## Consequences

- The 2022 code was deleted from the tree on 2026-10-21 (maintainer's call, to keep the public
  repo generic and current); tag `legacy-2022` marks the last commit containing it. Its
  rendering baseline lives in AGENTS.md.
- No cloud dependency may be added outside the optional sources above.
- Secrets live only on the Pi under /etc/lovebox and in gitignored local env files.
