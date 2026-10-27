# Technical notes

Sourced findings about the panel and the Pi toolchain. Re-verify anything dated before
relying on it; crates and vendor code move.

## 1. Rust on the display

- **Driver:** our own, in `lovebox/src/panel/waveshare.rs`, sending the command sequences
  from Waveshare's vendor drivers ([EPD_7in5_V2.c](https://github.com/waveshare/e-Paper/blob/master/RaspberryPi_JetsonNano/c/lib/e-Paper/EPD_7in5_V2.c),
  `epd7in5_V2.py` beside it). It replaced `epd-waveshare` on 2026-10-27, which had no busy
  timeout, no partial refresh and no way to send the fast-refresh registers.
- The V2 panel only updates BUSY after a status command (0x71), so the driver sends it
  before every poll. BUSY low means busy. spidev writes are chunked at 4096 bytes (a frame
  is 48,000 bytes).
- **HAL:** `rppal` 0.22.1 supports Zero W and Zero 2 W and implements embedded-hal 1.0 and
  0.2.7 traits for SPI, GPIO, and delay; the driver uses its plain SPI and GPIO API. Source: [docs.rs/rppal](https://docs.rs/rppal/latest/rppal/).
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
  The driver sends them when the "Fast refresh" setting is on (the default). First
  on-panel run: 4.5 s from wake to deep sleep, against 7.0 s before.

## 2. Toolchain for the Pi

| Board | Rust target | Tier | Notes |
|---|---|---|---|
| Pi Zero W (ARMv6, 32-bit only) | `arm-unknown-linux-gnueabihf` | Tier 2 with host tools | glibc ≥ 2.17; Bookworm ships 2.36, fine |
| Pi Zero W, static | `arm-unknown-linux-musleabihf` | Tier 2 | avoids glibc mismatch entirely |
| Pi Zero 2 W (Cortex-A53) | `aarch64-unknown-linux-gnu` | **Tier 1** | 64-bit Pi OS |

Source: [rustc platform support](https://doc.rust-lang.org/nightly/rustc/platform-support.html).

- Cross-compiling from the Mac, **verified 2026-10-21**:
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
