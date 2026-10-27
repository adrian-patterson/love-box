# History

How love-box got to where it is.

## 2022: the first version

Python on the Pi, a Flask API, a React app and Firebase. The Pi polled for a new message
or photo from a 10-minute cron job and drew it with Waveshare's Python driver: text in
Palatino at four sizes, photos cover-cropped without dithering. It ran for a few years,
then stopped working on current Raspberry Pi OS. The code is kept at git tag
`legacy-2022`.

## October 2026: the rewrite

The whole stack was replaced by one Rust binary running on the Pi, with no cloud account,
so the frame keeps working when the internet is down.

- **Plan (21 October).** Agent context, research notes and the Pi runbook. State moved
  to one JSON document on the Pi instead of a database
  ([ADR-002](docs/decisions/ADR-002-state-storage.md)).
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
  saved quotes in the library, least recently shown first. After a reboot it catches up once
  rather than replaying every missed day.
- **Slideshow (26 October).** A new card every interval, shuffled or in order, with a
  "Next" button on the Now screen. After a reboot the slideshow shows one card and
  carries on. The saved iPhone app's
  header no longer sits under the status bar.
- **Simpler modes (27 October).** After a day of use: a card you send by hand now stays
  up (sending pauses the slideshow or daily card), and the Now screen says what happens
  next with Start slideshow, Next and Pause. Settings save as you change them, with no
  Save button. Every photo and saved quote in the library is in the slideshow, so the
  per-item "In slideshow" switches and the add/remove bulk actions are gone. "Show now"
  on a photo and "Refresh now" show a spinner while the frame updates.
- **Fast refresh (27 October).** The panel's 3–4 flashes per change are its own built-in
  refresh, not extra work by the app. The driver library was replaced by our own code
  following Waveshare's, which can use their shorter "fast" refresh: on by default, with a
  Settings switch back to the standard one. An update went from 7.0 s to 4.5 s.
- **Feature complete (27 October).** Quiet hours, Google Photos import and AI-written notes
  were dropped from the roadmap, and the plan document with them. Old state files that
  still carry quiet-hours settings load and ignore them.
