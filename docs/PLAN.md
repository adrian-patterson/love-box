# love-box revamp plan

Status: **accepted by the maintainer 2026-10-21** (board + three defaults). Once accepted, this file is the
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

- Keeping the 2022 implementation runnable. It is gone from the tree; see tag `legacy-2022`.

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
2. **Photo source priority: DECIDED — phone upload (M3), then Google Photos Picker import
   (M4, promoted on 2026-10-21 because the maintainer is on iPhone with all photos in Google
   Photos), Immich optional.** True album-follow is not possible via Google's API since
   March 2025; see docs/RESEARCH.md §1–2.
3. **Re-image the SD card: DECIDED — yes.** Clean 32-bit Raspberry Pi OS Lite, current
   stable release. The card has not been flashed yet as of 2026-10-21; the Pi is not on Wi-Fi.
4. **AI features: DECIDED — start with the daily note and photo captions** (Ideas #1 and #2),
   each as its own unit in M6, behind config flags.

5. **Quote limit: DECIDED 2026-10-21 — 2,000 characters**, with 40 px margins and a 14 px minimum. Reject layout overflow explicitly; never truncate quotes. Board D-009 records the maintainer’s choice.

## Target architecture

```
phone / laptop ──http://lovebox.local──▶ lovebox (Rust, on the Pi)
                                           ├─ web UI (embedded static files, Vite+React+TS)
                                           ├─ JSON API (axum)
                                           ├─ state: /var/lib/lovebox/state.json + photos/ (ADR-002)
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
```

The 2022 Python/React/Firebase code was removed from the tree on 2026-10-21 (tag
`legacy-2022` holds the last commit with it). Its rendering baseline is recorded in
AGENTS.md; nothing else from it is needed.

## Where things stand (2026-10-26)

Work resumed under RESUME.handoff.01: finish the renderer, server, UI and compile-only panel support, one writer at a time. Stop before physical hardware access.

- **Accepted:** M0, U-101, U-201 renderer at `3089989` (D-011). All card types, photo ingest/crop/dither, CLI, fonts and render samples are complete; native tests/lint and ARMv6 cross-build/static check pass.
- **Memory evidence:** macOS separate-process peak RSS: 40 MP baseline JPEG 48.4 MiB, 3000×2000 RGB16 TIFF 100 MiB with a 160 MiB decode budget. The budget estimates decode buffers, not total process RSS. Large progressive JPEGs may be rejected conservatively; use the laptop import path once available.
- **Quote policy:** maximum 2,000 normalised characters; 40 px margins, 14 px floor. Layout overflow remains an explicit error, including unbreakable words.
- **Server accepted:** U-301 at `fa57a76`: axum API, atomic JSON state, bounded photo library/import, preview/current PNG, local fonts, settings/quotes and timed display worker. Native tests/lint and ARMv6 build/static checks pass; HTTP smoke at `d8b9b19` remains applicable after the default-only lint repair. Attempt 01 ended at a usage limit; attempt 02 completed validation (D-012/D-013).
- **Web UI accepted:** U-302 at `d7f2ca7`: local embedded React remote, all core screens, server PNG previews, library/settings/status. Native tests/lint and ARMv6 build/static checks pass; browser workflows, mobile/desktop light/dark, keyboard and 320 px/200% text checks pass. Bundle 82.62 kB gzip; D-014.
- **Panel compile-only accepted:** U-202 at `652016a`: hardware feature, pinned Waveshare V2/RPPAL driver, clear-on-wake/full refresh/deep sleep and 45 s initialization/update bounds. Native/fake-driver tests, hardware-feature checks, ARM clippy/cross-build/static v6 pass; no C build dependencies. D-015.
- **Resumption complete:** U-201, U-301, U-302 and U-202 compile-only accepted and integrated. Next actionable work is U-102/U-103 physical setup and on-panel M2/M3 acceptance; M4+ remains outside this scope.
- **M1 accepted 2026-10-26:** U-102 flashed Raspberry Pi OS Lite 32-bit 2026-10-06 (Trixie) via Imager CLI and cloud-init; PI-SETUP §0 corrected (no `enable_ssh` key, `rpi.interfaces.spi`, singular `user:`). U-103: first boot ~13 min; Zero W Rev 1.1 confirmed, `spidev0.0` present, key-only SSH, passwordless sudo; full-upgrade, §3 hardening (15 s hardware watchdog, unattended-upgrades, journald 50M), `provision.sh` and `make deploy` all ran first time. `lovebox` at `5b3ef01` serves `/healthz` and the UI on `http://lovebox.local`, panel state `ready`, ~6.7 MB RSS, and restarts at boot.
- **First on-panel test 2026-10-26:** the panel works end to end. Fixed: inverted image (V2 wants 1 = black on the wire), squashed library thumbnails (rebuilt at startup), and UI issues from phone testing; added a crop box on the photo screen. The white pre-clear was dropped (one full refresh per update, about 3 flashes); watch for ghosting and add a periodic deep clean if it appears. Quiet hours settings are hidden until M5.
- **Daily mode 2026-10-26:** the first M5 piece (see M5).
- **Slideshow mode 2026-10-26:** done (see M5). Quiet hours remain.
- **Next:** on-panel M2/M3 acceptance (U-202 on hardware): send a message, quote, photo and clear from the UI and confirm the panel, refresh time and deep sleep.
- **Backlog:** M4 Google Photos, M5 scheduler, M6 AI, M7 ops remain outside this resumption.

## Milestones

Each milestone ends with an acceptance check the maintainer can do from a phone or a terminal.
One writer at a time; a reader only for named questions. Model policy (maintainer, 2026-10-22): Luna high for bounded work, Luna extra high for cross-cutting server/state work; escalate only with evidence, rather than using Sol 6.1 by default. Board at
`.tmp/agents-messaging-board/`.

### M0 — Context and plan accepted  ✅ 2026-10-21
Deliverables: AGENTS.md, docs/, board, decisions D-001..D-003. Accepted by the maintainer.

### M1 — Pi reachable, provisioned, and deployable  ✅ 2026-10-26
Units: U-101 Mac toolchain + `lovebox` crate skeleton + cross-build + deploy scripts —
**accepted 2026-10-21 (D-004)**; U-102 flash SD card — **done 2026-10-26**; U-103 first SSH, provision per
docs/PI-SETUP.md, deploy — **done 2026-10-26**, deploying the full `lovebox serve` binary
rather than a hello-world.
- SSH key access to the Pi; clean OS; SPI enabled; `lovebox.local` resolves.
- Rust toolchain on the Mac (`rustup`), cross target installed, `deploy/deploy.sh` ships a
  hello-world binary and runs it under systemd.
- Acceptance: `ssh lovebox.local` works; `systemctl status lovebox` is active; journal shows
  the hello binary's log line.

### M2 — Renderer and panel  *(software accepted; on-panel acceptance pending U-103)*
- `lovebox` crate gains the `Panel` trait, `PngPanel`, and (U-202, compile-only until the Pi
  is up) `WavesharePanel`.
- **Renderer** produces the 800x480 1-bit `Frame` for every card type:
  - *Message*: the 2022 sizes and wrap widths (AGENTS.md "Baseline"), left-aligned.
  - *Quote*: body text plus optional attribution line. **Font is configurable** from a
    bundled set of SIL OFL faces (at least one classic serif, one modern serif, one
    readable sans). **Size fits the content automatically**: the renderer searches for the
    largest size at which the wrapped text fits the frame with margins, from a generous
    maximum for a one-liner down to a floor around 14 px for a page of prose. Alignment
    left or centred. **Maximum length 2,000 characters**; longer
    input is rejected by the API with a clear message, never silently truncated.
  - *Photo*: decode JPEG, PNG, TIFF (8- and 16-bit), BMP, WebP; cover-crop to 800x480
    (optionally honour a focal point); grayscale with contrast normalisation; dither to
    1-bit (Floyd–Steinberg default, Atkinson available). Sources are **very high
    resolution** (tens of megapixels, TIFFs included) so decoding runs under explicit
    memory limits sized for the Pi's 512 MB, and the server's upload path stores the
    original only transiently: the library keeps a bounded ~1600x960 copy plus the
    rendered frame. HEIC is not decoded on the Pi; iOS converts to JPEG at upload time,
    and the laptop import CLI (M3) converts anything else.
  - *Clear*: an all-white frame.
- `lovebox render {message|quote|photo|clear} ... -o out.png` CLI for development and
  golden tests; the same code path backs `/api/preview` in M3.
- Acceptance (U-201, on a laptop): golden-frame tests pass for each card type; a 40 MP
  JPEG and a 16-bit TIFF render under the memory limit; a 2,000-character prose quote renders
  at the floor size and a 12-word quote renders large. (U-202, on the Pi, later): a photo
  and a 48 pt message appear on the panel, no hang, panel sleeps after.

### M3 — Local web app replaces the old stack  *(software accepted; phone-to-panel acceptance pending U-103)*
- axum server, JSON-document store (ADR-002), photo library with upload (multipart;
  JPEG/PNG/TIFF/WebP; iOS converts HEIC on upload), a `lovebox import` laptop CLI that
  resizes huge TIFF/JPEG files locally and POSTs them, card queue, pinned mode,
  `/api/preview`, `/healthz`.
- UI: message composer with live preview, photo grid with upload, "showing now" header,
  status page (uptime, last refresh, disk, temperature). Screens, tokens, states, and what
  the UI needs from the API are specified in [UI-DESIGN.md](UI-DESIGN.md).
- Acceptance: from a phone on Wi-Fi, send a message and upload a photo; the panel updates
  within 10 seconds of tapping send.

### M4 — Google Photos import *(promoted ahead of slideshow on 2026-10-21)*

Why this comes right after the web app: the maintainer's photos live in Google Photos and
the phone is an iPhone, where "save to phone" from the Google Photos app downloads
full-size originals and is slow. The Pi must fetch the photos itself. The Library API
cannot do it: since 31 March 2025 its `albums.list`, `mediaItems.list`, and
`mediaItems.search` return only content the calling app created
(https://developers.google.com/photos/library/reference/rest), and the read-library scopes
were removed. The Picker API is the only official route left.

How it works (Google's Picker API; the Library API no longer exposes a user's albums to
third-party apps, see docs/RESEARCH.md §1):

1. **One-time setup, on the laptop.** Create a Google Cloud project, enable the Photos
   Picker API, create an OAuth client (type "Desktop"), and run `lovebox google-auth`
   natively: it opens the Google consent page in the browser, receives the code on a
   loopback redirect, and writes a refresh token to a file. Copy that file to
   `/etc/lovebox/` on the Pi. The Pi cannot do this step itself: Google's device-code flow
   does not allow the Photos scope, and Google rejects plain-http redirect URIs such as
   `http://lovebox.local/...`. Scope: `photospicker.mediaitems.readonly` only.
2. **Picking, from the phone.** In the lovebox UI tap "Add from Google Photos". The Pi
   mints an access token from the refresh token, creates a Picker session, and shows the
   session's `pickerUri` as a button and a QR code. Opening it lands in Google Photos
   (app or web) with a picker: select photos, or open an album and select all, then Done.
3. **Download, on the Pi.** The Pi polls the session until `mediaItemsSet`, lists the picked
   items (100 per page), and downloads each `baseUrl` with `=w1600-h960` (bounded, aspect
   preserved; our renderer does the cover-crop and dither) and an `Authorization: Bearer`
   header. `baseUrl`s expire after 60 minutes, so download immediately, then delete the
   session. The photos are now ordinary library entries; showing them never touches Google
   again. The picked item `id` is stable across sessions and is stored to skip duplicates.
4. **Adding more later** means picking again. There is no "follow this album": Google
   removed that for third-party apps in March 2025. Anyone wanting automatic sync should
   use Immich or a shared folder instead.

Consent-screen status is the open risk: while the Cloud project is in "Testing", refresh
tokens expire after 7 days (re-run step 1 weekly); publishing it "In production" removes
that expiry but, if Google classes the Picker scope as sensitive, shows an "unverified app"
warning on consent that the owner clicks through once. Acceptable for a one-household
frame; the spike settles which it is.

Acceptance: pick five photos on the phone, they appear in the library within a minute; a
second pick of the same photos adds nothing; the token still works 8 days after setup (or
the doc says exactly what to re-run).

Optional unit, built only if the token-status answer is bad: **shared-link importer**,
labelled experimental in the UI. Paste a Google Photos "share album" link; the Pi scrapes
the album page for preview-size image URLs and imports them. No OAuth, whole albums at
once, breaks whenever Google changes the page. Never the default path.

### M5 — Slideshow, scheduling, and quotes
- **Slideshow mode**: cycles through a set (all photos, or a tag/album the user picks) in
  shuffle or upload order. Interval is set in the UI: default 15 min; the normal control
  ranges from 180 s (Waveshare's recommended minimum between refreshes) to 24 h; an
  "advanced" toggle unlocks 15 s–180 s with a visible warning about panel wear and the
  4 s flash on every change. Interval, order, set, and quiet hours persist in the state
  document and
  survive restarts; the scheduler catches up sensibly after a reboot (shows the next item,
  does not burst through missed ones).
- Daily mode (new card at a set time) — **done 2026-10-26**: `lovebox::scheduler` checks
  every 30 s; once `settings.daily.time` (local) has passed and `state.last_daily_on` is
  not today, it shows the least recently shown in-rotation photo or saved quote through
  the normal show path. Catch-up after a reboot is one card; switching to Daily after
  today's time waits until tomorrow; a failed update retries after 10 min.
  `capabilities.daily` and `next_change_at` report it.
- Slideshow mode — **done 2026-10-26**: the next card is due one interval after
  `state.last_refresh_at` (any change, so a card sent by hand stays up a full interval;
  a reboot shows one card, then carries on). In order takes the least recently shown
  in-rotation item; shuffle picks at random from the less recently shown half, so nothing
  repeats within half a cycle. `set` is `all` (photos and saved quotes in rotation) or
  `tag:<name>` (photos with that tag). A one-item slideshow does not redraw.
  `POST /api/slideshow/next` shows the next card now; `capabilities.slideshow` and
  `next_change_at` report it. The UI follows UI-DESIGN (amber note under 3 min) rather
  than an "advanced" toggle. Pause is choosing Pinned. Still to do: quiet hours (no refresh in a nightly window; the
  panel sleeps), a curated quotes list editable in the UI, quote render style (typographic
  layout, author line).
- Acceptance: set "slideshow every 3 min" with 5 photos, observe 3 changes in 9 min in the
  right order; set "every 15 s" via advanced, see the warning and the fast cycle; set a
  quiet window covering now, observe no change; set a quote, see it rendered.

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
3. **Ask the frame.** Type "something from the beach trip" or "a cosy one" in the UI;
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

- **Google OAuth app lifecycle** (unverified apps, token expiry). Contained in M4 as a spike.
- **ARMv6 toolchain friction** if staying on the Zero W. Mitigation: musl static target.
- **Panel driver edge cases** (busy-wait with no timeout, wake-after-sleep ghosting).
  Mitigation: our own timeout wrapper, a full refresh on every update, hardware test in M2 before anything
  else is built on it.
- **SD card wear.** Mitigation: journald cap, few writes, backups.
