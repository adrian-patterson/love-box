# History

How love-box got to where it is. For what is planned next, see
[docs/PLAN.md](docs/PLAN.md); for the reasoning behind big choices, see
[docs/decisions/](docs/decisions/).

## 2022: the first version

Python on the Pi, a Flask API, a React app and Firebase. The Pi polled for a new message
or photo from a 10-minute cron job and drew it with Waveshare's Python driver: text in
Palatino at four sizes, photos cover-cropped without dithering. It ran for a few years,
then stopped working on current Raspberry Pi OS. The code is kept at git tag
`legacy-2022`.

## October 2026: the rewrite

The whole stack was replaced by one Rust binary running on the Pi, with no cloud account,
so the frame keeps working when the internet is down.

- **Plan and decisions (21 October).** Agent context, the plan, research notes and the Pi
  runbook. State moved from Firebase to one JSON document on the Pi (ADR-002). Google
  Photos became an import through Google's Picker API, because third-party apps can no
  longer follow albums.
- **Renderer (21–22 October).** Message cards at the 2022 sizes in bundled open-licence
  fonts, quotes that auto-fit up to 2,000 characters, and photos decoded under a memory
  budget, cropped and dithered to 1-bit. Golden-frame tests compare the output pixel for
  pixel.
- **Server and web app (22 October).** The local API, atomic state store, photo library
  and laptop import, plus a React web app embedded in the binary and served at
  `http://lovebox.local`. Previews come from the same renderer as the panel.
- **Panel driver (22 October).** Waveshare 7.5" V2 support behind a `Panel` trait, with
  timeouts, so everything else still builds and tests on a laptop.
- **On the hardware (26 October).** The Pi was flashed with Raspberry Pi OS Lite using
  cloud-init, hardened (key-only SSH, automatic security updates, hardware watchdog) and
  provisioned, and the service was deployed with one command. The first on-panel test
  found and fixed:
  - an inverted image (the V2 panel expects 1 = black)
  - squashed photo thumbnails
  - several layout issues on phones

  Each update now does a single full refresh, with half the flashing of the 2022 clear
  plus redraw. The photo screen gained a crop box you can drag and resize to zoom.
- **Daily mode (26 October).** A new card each day at a set time, from the photos and
  saved quotes in rotation, least recently shown first. After a reboot it catches up once
  rather than replaying every missed day.
- **Slideshow (26 October).** A new card every interval, shuffled or in order, with a
  "Next" button on the Now screen. A card sent by hand stays up for a full interval, and
  after a reboot the slideshow shows one card and carries on. The saved iPhone app's
  header no longer sits under the status bar.
