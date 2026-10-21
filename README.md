# love-box

An 800x480 e-paper picture frame you control from your phone. Waveshare 7.5" e-paper HAT
(V2), a Raspberry Pi Zero W, and a web page on your home network for sending a message or
picking a photo. Built as a gift for leaving notes for someone you love.

## Project direction (October 2026)

The project is being rewritten. The 2022 version (still in this repo, described under
"Legacy implementation" below) needed a Firebase database, a Flask API and a React build
hosted somewhere else, and a cron job that checked for changes every 10 minutes. It no
longer runs on current Raspberry Pi OS because its Python GPIO driver is gone.

The new version is **one Rust binary that runs on the Pi, with no cloud account**:

- Reachable on your LAN at `http://lovebox.local`; changes show on the panel in seconds.
- Content sources: a typed message, a photo library you upload to from your phone, photos
  picked from **Google Photos** (via Google's Picker API; following an album automatically is
  no longer possible for third-party apps), curated quotes, and optional AI-written daily
  notes and photo captions via the Claude API.
- Everything develops and tests on a laptop without hardware: the panel sits behind a trait
  with a PNG implementation, and the UI preview is the exact frame the panel will show.
- One-command deploy, systemd service, health page, backups.

Status: milestone **M1** (toolchain, cross-build, deploy kit) is done on the laptop side;
the Pi provisioning and the display driver come next. Nothing from the rewrite drives a
panel yet. Progress and the full design live in [docs/PLAN.md](docs/PLAN.md).

### Where to start

| You want to... | Read |
|---|---|
| Understand the design and roadmap | [docs/PLAN.md](docs/PLAN.md) |
| Set up a Pi Zero W from a blank SD card | [docs/PI-SETUP.md](docs/PI-SETUP.md) |
| Build, test, cross-compile, deploy | `make test`, `make build-pi`, `make deploy`; details in [deploy/README.md](deploy/README.md) |
| Know why Google Photos works the way it does, or which crates drive the panel | [docs/RESEARCH.md](docs/RESEARCH.md) |
| See the decisions that bind the rewrite | [docs/decisions/](docs/decisions/) |

### Using this repo with an AI coding agent

[AGENTS.md](AGENTS.md) is the entry point: it tells an agent what the project is, which
parts are legacy, the rendering rules the rewrite must preserve, the commands that exist,
and the rules for working here. Clone the repo, open it in Claude Code or any agent that
reads `AGENTS.md`, and point it at the next unfinished milestone in `docs/PLAN.md`. Secrets
(Wi-Fi, OAuth client, API keys) never belong in the repo; the docs say where they go.

## Hardware

Unchanged from the original build and still what the rewrite targets:

- [Waveshare 7.5" e-Paper HAT](https://www.waveshare.com/7.5inch-e-paper-hat.htm), V2 panel, 800x480, black and white
- Raspberry Pi Zero W (the original, ARMv6; a Zero 2 W also fits and is faster)
- A 7"x5.5" picture frame, a 3D-printed case for the Pi and HAT, and a 5 V / 2.5 A micro-USB supply
- A microSD card (8 GB is enough; buy one that ships with an SD adapter for flashing)

## Legacy implementation (2022)

Everything below describes the original Python + React + Firebase version. It is kept for
reference until the rewrite replaces it, is not maintained, and does not run on current
Raspberry Pi OS. Do not follow its setup steps for a new build; use
[docs/PI-SETUP.md](docs/PI-SETUP.md) instead.

- A remotely controllable e-paper display using waveshare's 7.5" e-paper display, a Raspberry Pi Zero W, and Firebase's free-tier real-time database
- A gift meant for leaving meaningful notes/messages for someone you love

## Repository structure (legacy)

### `/Api`

- Contains a simple flask API to serve requests for updating the message/picture for the display
- `message.py` serves as a class  representing the database objects
- `message_repository.py` serves as a repository for fetching and updating database values
- `api.py` serves requests for updating or previewing messages/images from the UI (in the following section)


---

### `/LoveBoxUi`

- Contains a React application for easily changing the display picture 
- The look:
![image](https://user-images.githubusercontent.com/47571939/167021667-e28bef4f-c37d-4e7e-863e-f9327ca73744.png)
- Or on mobile:

![image](https://user-images.githubusercontent.com/47571939/167021748-930d9bcb-1317-4bc5-859c-ef41181d8780.png)
- The UI allows: 
  - Viewing the picture gallery, choose a picture and upload. Pictures are shuffled by default and can be reshuffled
  - Selecting a font size and uploading a message (send button used for uploading)
  - Selecting a font size and previewing where line breaks will be in message (with left preview button)


---

### `/RpiUpdateService`

- Update service hosted on the raspberry Pi Zero W which controls the display
- Python script which is run every 10 minutes to check database contents and update display accordingly
- Script is called using a cronjob

#### `/ShellScripts`

- `auto_update.sh`
  - Called every 24 hours using a cronjob to update raspberry pi with remote code
- `update_display.sh`
  - Called every 10 minutes using a cronjob to update the display with database contents
- `install.sh`
  - Installs needed dependencies for the raspberry pi

#### `/src`
- `update_display.py` updates the display itself with the contents within the database
  - The time stored within the database is compared with the current time
  - If the database was last updated in the previous 10 minutes, then the display is changed
- `format_images.py` script allows one to drop JPEG images into the `/pic` library, and then transforms them to the dimensions required by the display (800x480)
  - These images are also saved to the `/public` directory for the react app to be displayed on the website
- `message.py` and `message_repository.py` are the same as found in the API
- `/lib`
  - All waveshare libraries
        - These are taken right from the [waveshare/e-paper repo](https://github.com/waveshare/e-Paper)
        - Contains a c shared library and python example for writing to the 7.5" display
- `/pic`
  - Where pictures are stored and read from
- `/font`
  - Stores font for display

## Crontab entries (legacy)

- `*/10 * * * * /home/pi/projects/love-box/RpiUpdateService/ShellScripts/update_display.sh >> /home/pi/logs/update_display_log.txt 2>&1`
  - Ran every 10 minutes
- `0 0 * * * /home/pi/projects/love-box/RpiUpdateService/ShellScripts/auto_update.sh >> /home/pi/logs/git_log.txt 2>&1`
  - Ran once a day
- Keep in mind that these paths are relative and will have to be adjusted based on where the repository files are stored on the raspberry pi

## Photos of the build
### Displaying a Picture
![image](https://user-images.githubusercontent.com/47571939/163594413-c6eb3808-4513-4128-835d-d810d2fb4636.png)
### Displaying Text
![image](https://user-images.githubusercontent.com/47571939/163594460-b78e4632-db6d-4be3-b008-e5df3094526b.png)
### The Wiring
![image](https://user-images.githubusercontent.com/47571939/163594511-85c280f8-d6a0-4ccc-a2cf-58c4714fc34f.png)
![image](https://user-images.githubusercontent.com/47571939/163594503-563f6b02-9094-4581-a29f-ac31b20f1bf2.png)
### The Case Designed for the Pi + Waveshare Pi-HAT
![image](https://user-images.githubusercontent.com/47571939/163594571-d0f96c77-cdce-4f6a-8a29-8d045d3c6322.png)
![image](https://user-images.githubusercontent.com/47571939/163594608-162ef34d-f450-4de3-981d-98be82ceefe8.png)
![image](https://user-images.githubusercontent.com/47571939/163594590-a1441343-8cd4-471c-af65-96cdf93154f1.png)


