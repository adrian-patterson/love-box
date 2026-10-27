# Research notes (2026-10-21)

Findings that shaped the plan, each with its source. Re-verify anything dated before
relying on it in a unit; APIs and crates move.

## 1. Google Photos: can the frame follow an album?

**Verdict: not via the official API. Picking photos is supported; following an album is not.**

- Since 31 March 2025 the Library API only lists, searches, and retrieves media *your app
  created*. The broad `photoslibrary` and sharing scopes were removed. Checked again
  2026-10-21: the reference page now describes `albums.list`, `mediaItems.list`, and
  `mediaItems.search` as returning only content "your app created"
  (https://developers.google.com/photos/library/reference/rest).
  Sources: [Google Photos API updates](https://developers.google.com/photos/support/updates),
  [Developers Blog: Picker API launch](https://developers.googleblog.com/en/google-photos-picker-api-launch-and-library-api-updates/).
- The replacement is the **Picker API**: the app creates a session, the user opens a
  Google-hosted `pickerUri` on their phone, selects photos (or a whole album's contents),
  the app polls `mediaItemsSet`, lists the picked items, and downloads each `baseUrl`.
  Scope: `https://www.googleapis.com/auth/photospicker.mediaitems.readonly`. Page size max 100.
  Sources: [Picker get started](https://developers.google.com/photos/picker/guides/get-started-picker),
  [mediaItems.list](https://developers.google.com/photos/picker/reference/rest/v1/mediaItems/list).
- Picked items are a snapshot: new photos added to the album later are not seen until the
  user picks again. `baseUrl`s are short-lived, so download immediately.
  Source: [third-party frame guide](https://devdad.kr/blog/google-photos-android-photo-frame.html) (unofficial; the 2,000-per-session figure there is unconfirmed).
- Unofficial route: scraping a *public shared album link* works today with tools like
  [scrape-google-photos](https://github.com/alexcrist/scrape-google-photos) and is planned in
  [brotherlogic/rose#87](https://github.com/brotherlogic/rose/issues/87). It breaks whenever
  Google changes markup and has terms-of-service risk. Treated as a last resort.
- Open risk for the Picker route: OAuth consent for an unverified app in "testing" mode
  expires refresh tokens after 7 days. Needs a spike to confirm whether the Picker scope
  is classed as non-sensitive (no verification needed) and whether "production" mode is
  available without verification for a single-user app. Resolved in milestone M4.

**Practical implication:** the UX "pick photos on your phone and they appear on the frame"
is achievable. The UX "add to this album and the frame picks it up on its own" needs a
non-Google source (Immich, a shared folder) or the fragile scraper.

## 2. Alternatives to Google Photos as a source

- **Upload from the phone through the frame's own web UI.** Zero dependencies, works
  offline, and on iOS/Android a PWA can appear in the share sheet. Chosen as the baseline.
- **Immich** (self-hosted photo server) has a stable API: API keys scoped to
  `album.read`/`asset.read`, album listing and asset download endpoints. One open-source
  frame already pulls from it with weighted pools (on-this-day, favourites, recent).
  Sources: [Immich API overview](https://blog.elest.io/immich-api-build-custom-photo-workflows-and-automate-your-library/),
  [Gladys integration thread](https://community.gladysassistant.com/t/integration-immich/10328).
  Optional; only worth it if you already run Immich or want true album sync.
- **iCloud shared album public link** has an undocumented JSON webstream endpoint that
  several frame projects use. Same fragility caveat as Google scraping.
  Sources: [ben.page/eink](https://ben.page/eink), [Hackster write-up](https://hackster.io/news/old-nook-ebook-reader-becomes-icloud-photo-album-frame-7521b642a267).

## 3. Rust on the display

- **Driver (superseded 2026-10-27 by our own, see "Why one refresh still flashes" below):**
  `epd-waveshare` (rust-embedded-community) has `epd7in5_v2`
  (800x480, `Display7in5`, `Epd7in5`). v0.6.0 released 2024-10-28.
  Sources: [docs.rs](https://docs.rs/epd-waveshare/latest/epd_waveshare/),
  [CHANGELOG](https://github.com/rust-embedded-community/epd-waveshare/blob/main/CHANGELOG.md).
- The V2 panel only updates BUSY after a `GetStatus` (0x71) command. Current `main`
  handles this: `wait_until_idle_with_cmd` sends the status command before every poll,
  and Linux SPI writes are chunked at 4096 bytes (a frame is 48,000 bytes).
  Verified in [interface.rs](https://raw.githubusercontent.com/rust-embedded-community/epd-waveshare/main/src/interface.rs)
  and [epd7in5_v2/mod.rs](https://raw.githubusercontent.com/rust-embedded-community/epd-waveshare/main/src/epd7in5_v2/mod.rs).
  Caveats: no timeout in the busy loop (a wedged panel hangs the thread; wrap it with our
  own timeout), `update_partial_frame` and `set_lut` are `unimplemented!()`. Partial refresh
  is therefore out of scope for the first version. Older issue
  [caemor/epd-waveshare#70](https://github.com/caemor/epd-waveshare/issues/70) describes the
  pre-fix hang; pin the crate to a commit that includes the status-command fix.
- **HAL:** `rppal` 0.22.1 supports Zero W and Zero 2 W and implements embedded-hal 1.0 and
  0.2.7 traits for SPI, GPIO, and delay. Source: [docs.rs/rppal](https://docs.rs/rppal/latest/rppal/).
- **Working precedent:** a Pi Zero W + 7.5" V2 photo frame in Rust using exactly this stack:
  [DIY E-Ink Photo Frame](https://blog.arranfrance.com/post/diy-eink-photo-frame/).
- **Dithering:** `image` crate for decode/resize, plus `dither` or `dithers` for
  Floyd–Steinberg to 1-bit; `eink-image` shows the full pipeline (gamma, contrast,
  diffusion factor). Sources: [eink-image](https://docs.rs/eink-image),
  [dither](https://docs.rs/crate/dither/latest), [dithers](https://docs.rs/dithers).
- **Refresh hygiene:** full refresh takes ~4 s and flickers; Waveshare recommends a clear
  after waking from deep sleep. Practice is full refresh every change for a frame that
  changes a few times a day, and deep sleep between updates.
  Sources: [7.5inch e-Paper HAT Manual](https://www.waveshare.com/wiki/7.5inch_e-Paper_HAT_Manual),
  [Waveshare ESP32 e-paper notes](https://docs.waveshare.com/ESP32-Peripheral-Tutorials/Display/E-Paper).
- **Why one refresh still flashes several times (checked 2026-10-27):** the black/white
  flashes are the full-refresh waveform stored in the panel itself; we send one refresh
  command per change. Waveshare's `EPD_7IN5_V2_Init_Fast` selects a shorter waveform
  (after `0x00: 0x1F`, `0x50: 0x10 0x07`, power on: `0x06: 0x27 0x27 0x18 0x17`,
  `0xE0: 0x02`, `0xE5: 0x5A`, i.e. it pins the temperature the waveform is chosen by).
  `epd-waveshare` has no public way to send those bytes, so on 2026-10-27 it was replaced by
  our own driver on `rppal` (`lovebox/src/panel/waveshare.rs`), with fast refresh as a
  setting. First on-panel run: 4.5 s from wake to deep sleep, against 7.0 s before. Source: [EPD_7in5_V2.c](https://github.com/waveshare/e-Paper/blob/master/RaspberryPi_JetsonNano/c/lib/e-Paper/EPD_7in5_V2.c).

## 4. Toolchain for the Pi

| Board | Rust target | Tier | Notes |
|---|---|---|---|
| Pi Zero W (ARMv6, 32-bit only) | `arm-unknown-linux-gnueabihf` | Tier 2 with host tools | glibc ≥ 2.17; Bookworm ships 2.36, fine |
| Pi Zero W, static | `arm-unknown-linux-musleabihf` | Tier 2 | avoids glibc mismatch entirely |
| Pi Zero 2 W (Cortex-A53) | `aarch64-unknown-linux-gnu` | **Tier 1** | 64-bit Pi OS |

Source: [rustc platform support](https://doc.rust-lang.org/nightly/rustc/platform-support.html).

- Cross-compiling from the Mac, **verified 2026-10-21 (U-101)**:
  `cargo zigbuild --release --target arm-unknown-linux-gnueabihf.2.36` (rustc 1.99.1,
  cargo-zigbuild 0.24.0, zig 0.17.0 from PyPI `ziglang` in a venv) yields
  `Tag_CPU_arch: v6`, `Tag_FP_arch: VFPv2`, max `GLIBC_2.34`, 1.3 MB stripped, ~11 s cold.
  cargo-zigbuild already passes `-mcpu=generic+v6+strict_align+vfp2-d32` to zig for this
  target; `lovebox/.cargo/config.toml` additionally pins `target-cpu=arm1176jzf-s`.
  `make check-pi` proves ARMv6 with `readelf -A`; an emulated run under Docker's qemu-arm is
  not an ARMv6 proof (it executes ARMv7 code too). Docker on this Mac is the brew CLI with
  colima (not Docker Desktop); ARM containers need
  `docker run --privileged --rm tonistiigi/binfmt --install arm` after each VM start.
  `cross` and the musl target were never needed. An armv7 build segfaults on a Zero W.
  Sources: [cross-rs#426](https://github.com/rust-embedded/cross/issues/426),
  [Rust forum thread](https://users.rust-lang.org/t/cross-compile-ubuntu-19-04-raspbery-pi-zero-w/28530),
  [ZeroClaw Pi Zero W build notes](https://git.polymech.info/polymech/zeroclaw/src/tag/v0.1.8/docs/hardware/raspberry-pi-zero-w-build.md).
- Building *on* the Zero W is possible but slow (single ARM11 core, 512 MB). Cross-compile.
- **Zero 2 W** is a drop-in replacement (same form factor, same HAT), quad-core 64-bit,
  roughly 5x multi-thread and 1.5–2.5x single-thread faster, 512 MB RAM. It moves us to a
  Tier 1 Rust target and a 64-bit OS. Sources: [picockpit](https://picockpit.com/raspberry-pi/?p=928),
  [etechnophiles comparison](https://etechnophiles.com/rpi-zero-2w-vs-zero-w-difference-design-spec/).

## 5. Why the Python stack is dead on a fresh Pi

`RPi.GPIO` does not work on Raspberry Pi OS Bookworm (the kernel sysfs GPIO interface
is gone); the shim is `rpi-lgpio`, and Bookworm requires Python packages in a venv
(PEP 668). Source: [DietPi forum](https://dietpi.com/forum/t/node-red-gpio-not-working-after-upgrade-to-9-10/22806/5)
(forum claim; consistent with Waveshare moving its examples to `gpiozero`).

## 6. Prior art for "AI on an e-ink frame"

All Python, all useful as feature inspiration, none as code:
[InkyPi](https://github.com/fatihak/InkyPi) (plugins: AI text, weather, calendar),
[MagInkDash](https://github.com/speedyg0nz/MagInkDash) (calendar + weather + ChatGPT),
[kuvastin](https://github.com/Iletee/kuvastin) (daily AI art from your calendar).
