# UI design — the phone web app

The design contract for the web UI (milestone M3, unit U-302). It says what the app looks
like, how it behaves, and what it needs from the server. The API shapes themselves belong
to `docs/API.md` (written by the server unit, U-301); where this file and the API disagree,
the API wins and this file is corrected.

Produced with the ui-ux-pro-max design guidance (style, palette, typography and UX rule
searches), then adjusted for this product; see "Recommendations not taken" at the end.

## What the app is for

A household opens `http://lovebox.local` on a phone, usually standing near the frame, to do
one of four things in under a minute: see what the frame shows, put a note or quote on it,
add photos, or change how it rotates. It is a remote control for a picture frame, not a
dashboard. Every screen is built around the frame preview.

Design principles, in priority order:

1. **Preview equals panel.** The UI never draws its own approximation of the frame. Every
   preview is the PNG the Rust renderer produced for the panel (`/api/preview` and the
   current-frame image). The preview is shown as-is, white paper and black ink, in both
   light and dark mode; it is never inverted, tinted, or smoothed.
2. **Paper and ink.** The UI echoes the e-paper: warm off-white paper, near-black ink, one
   warm accent, no gradients, no glass, no shadows beyond a hairline. Monochrome first;
   colour only for status and the primary action.
3. **One thumb, one hand.** Phone first (375 px wide), bottom navigation, 44 px touch
   targets, nothing that needs hover, nothing that needs a keyboard shortcut.
4. **Honest about the hardware.** An update takes about 4 seconds and flashes. The UI says
   so before and during an update, and warns, never blocks, when the slideshow interval
   is faster than the panel maker recommends.
5. **Works with the internet down.** Nothing loads from a CDN. Fonts, icons, and scripts are
   in the binary.

## Stack and platform constraints

| Concern | Choice |
|---|---|
| Framework | Vite + React + TypeScript, strict mode. Pin exact versions in `web/package.json` |
| Styling | Plain CSS with custom properties (tokens below) and CSS modules. No CSS framework |
| Routing | History API routes, one per screen (see Navigation). The server serves `index.html` for any non-API path |
| Data | `fetch` with small typed hooks; `AbortController` cancels stale previews. No global state library |
| Icons | Phosphor (`@phosphor-icons/react`), regular weight, 20 px in controls, 24 px in the tab bar. SVG only, never emoji |
| Fonts | UI text uses the system stack. The bundled OFL faces load only where content is previewed in them (font picker) |
| Bundle budget | Under 150 KB of gzipped JS and CSS for first load, excluding fonts. The Pi serves it over Wi-Fi |
| Browsers | iOS Safari (current and previous major), Android Chrome, desktop Safari/Chrome/Firefox |

`http://lovebox.local` is **not a secure context**. Browsers withhold some APIs from it, so
the UI must not depend on them:

- `crypto.randomUUID()` is unavailable: the server issues every id.
- `navigator.clipboard`, `navigator.share`, and service workers are unavailable: no offline
  cache, no share sheet, no copy buttons that rely on the Clipboard API.
- `createImageBitmap`, `<canvas>`, `fetch`, and file inputs work, so client-side resizing
  is possible.

Add-to-Home-Screen gets a web manifest, an `apple-touch-icon`, and
`apple-mobile-web-app-capable` so the app opens full-screen from an icon. Verify standalone
mode on an iPhone over plain HTTP before relying on it.

## Navigation

A bottom tab bar with four tabs, always visible except while the on-screen keyboard is up.
Each tab has an icon and a text label, the active tab has `aria-current="page"`, and the bar
sits above the home indicator with `env(safe-area-inset-bottom)` padding.

| Tab | Route | Purpose |
|---|---|---|
| Now | `/` | What the frame shows, frame status, quick actions |
| Write | `/write` | Compose a note or quote, preview it, show it |
| Library | `/library/photos`, `/library/quotes` | Photos and saved quotes; what is in rotation |
| Settings | `/settings`, `/settings/status` | Display mode, slideshow, quiet hours, sources, status |

Back and forward work: opening a photo or quote detail pushes a route
(`/library/photos/<id>`, `/write?quote=<id>`), and closing it goes back. Every screen is
deep-linkable.

On screens 900 px and wider the tab bar becomes a left rail and Write shows the preview and
the form side by side. Content width is capped at 560 px in single-column layouts.

## Screens

### Now

- **Status line** at the top: a dot and a word. Green "Ready", amber "Updating…", amber
  "Paused (quiet hours)", red "Not responding". Always paired with text, never colour
  alone. Shows the time of the last refresh ("Updated 2 min ago"), refreshed every 30 s.
- **The frame**: the current frame image at 5:3 in a thin bezel (1 px ink border, 12 px
  radius on the bezel, square corners on the image). Alt text describes the card ("Quote
  in EB Garamond, left-aligned").
- **Caption** under it: what it is and why it is there ("Photo · slideshow, next in 12
  min", "Quote · pinned").
- **Quick actions** as a row of secondary buttons: "Next" (only in slideshow mode),
  "Pause slideshow" / "Resume", "Clear frame" (confirm).
- **Empty state** on a fresh install: "Nothing on the frame yet." with two buttons, "Write
  something" and "Add photos".
- **Refresh moment**: when the frame finishes an update, the preview plays a short
  e-paper-style flash (ink, then paper, then the image, about 500 ms). Skipped under
  `prefers-reduced-motion`. This is the one decorative animation in the app.

### Write

The composer for notes and quotes. Top to bottom on a phone:

1. **Preview** at 5:3, the renderer's PNG for the current draft. Updates 600 ms after the
   last keystroke or style change; a newer edit aborts the in-flight request. While a new
   preview renders, the previous one stays visible with a small "Updating preview" label,
   so the layout never jumps. The Pi Zero is slow: assume 0.5–2 s per auto-fit render.
2. **Text area**, labelled "Message", auto-growing, 16 px or larger so iOS does not zoom.
   Under it, one helper line that changes with state:
   - Auto size: "Fits at 42 px · 412 / 3,000 characters".
   - Fixed size: "Size L fits about 26 × 7 characters" and, when the preview reports
     truncation, an amber "The end will be cut off" warning.
   - Over the limit: a red inline error, linked with `aria-describedby`, "That's 3,214
     characters; the maximum is 3,000." The Show button is disabled with the reason
     visible, never silently.
   The limit and the error text come from the server; 3,000 is the current default and
   may change (see the open question in PLAN.md "Where things stand").
   Pasting a page of a book is a normal case, not an edge case: the field accepts it, the
   counter and preview handle it, and blank lines are kept as paragraph breaks.
3. **Style**, always visible, not hidden in a menu:
   - **Font**: a horizontal row of chips, one per bundled face, each showing "Aa" and its
     name set in that face (fonts load lazily on first view). Selected chip has an ink
     border and a check icon.
   - **Size**: a segmented control, Auto · S · M · L · XL. Auto is the default and
     auto-fits the text (quote card, 14–120 px). S/M/L/XL are the four classic message
     sizes (30, 48, 64, 96 px), left-aligned, truncated with "…" when too long.
   - **Alignment**: Left · Centre. Enabled for Auto only; with a fixed size it is shown
     disabled with the note "Fixed sizes use the classic left-aligned layout."
   - **Attribution**: an optional single-line field, "Who said it", for Auto only, drawn
     under the text as "— name".
4. **Actions**: primary "Show on frame" (full width), secondary "Save to library". When
   the draft came from a saved quote, "Save" updates it and "Save as new" copies it.

Showing: the button becomes "Updating frame… about 4 s" with a spinner and stays disabled
until the server confirms. Success is announced in an `aria-live` region and a toast, "On
the frame", then the app moves to Now. A server error keeps the draft and shows the
server's message under the button with a Retry.

The exact text `clear` (the 2022 behaviour) shows the hint "This will blank the frame" and
the button reads "Clear frame".

### Library

A segmented switch at the top, **Photos · Quotes**, each its own route.

**Photos**

- "Add photos" button at the top opens the system picker (`<input type="file"
  accept="image/*" multiple>`). iOS converts HEIC to JPEG for this input.
- **Client-side resize** before upload, when the browser can decode the file: fit inside
  1600 × 960 with `createImageBitmap` and a canvas, encode JPEG at quality 0.9, upload
  that. When the browser cannot decode it (for example TIFF in Chrome), upload the
  original and let the server decode it under its memory limits. Never upload a 40 MB
  original from a phone when a 400 KB copy will do.
- **Upload queue** above the grid: one row per file with name, a determinate progress bar,
  and a final state. Errors stay on their row in plain language with the server's reason
  ("Too large: 120 megapixels, the maximum is 80. Resize it first, or import it from a
  laptop with `lovebox import`."). One failed file never stops the rest.
- **Grid**: three columns on a phone, square colour thumbnails (the server's small copy,
  lazy-loaded, fixed aspect ratio so nothing shifts). A small ink badge marks photos in
  the slideshow. Long lists paginate or virtualise beyond 200 photos.
- **Photo detail** (route `/library/photos/<id>`): the dithered frame preview at 5:3; tap
  the original thumbnail to set the focal point used by the cover crop, with the preview
  re-rendering; a Floyd–Steinberg / Atkinson choice labelled "Dither: Smooth / Crisp"; an
  "In slideshow" switch; "Show now"; "Delete".
- **Select mode** for bulk actions: "Select" in the header, checkboxes on thumbnails, then
  "Add to slideshow", "Remove from slideshow", "Delete".

**Quotes**

- A list of saved quotes: first line in the quote's own font, attribution and font name
  as muted second line, an "In rotation" switch on the right.
- Tap a quote to open it in Write (`/write?quote=<id>`). "New quote" opens an empty Write.

**Deleting** (photos and quotes): the item disappears at once and a toast offers "Undo" for
5 seconds; the client sends the delete when the toast expires. Bulk delete of more than
five items asks for confirmation first instead.

### Settings

Grouped sections, each a card with a heading.

- **Display mode**: three radio cards with one-line explanations. *Pinned*: "Keeps the
  current card until you change it." *Slideshow*: "Cycles through photos and quotes in
  rotation." *Daily*: "A new card each morning." Modes the server does not support yet are
  shown disabled with "Coming soon".
- **Slideshow**: interval as a select of presets (15 s, 30 s, 1 min, 2 min, 3 min, 5 min,
  15 min, 30 min, 1 h, 3 h, 6 h, 12 h, 24 h), default 15 min. Choosing anything under
  3 min shows an amber inline note, not a dialog: "Faster than every 3 minutes means a
  visible flash each time and wears the panel sooner. The panel maker recommends at least 3
  minutes." Below it: order (Shuffle · In order) and the server-supported selection scope (`all` or a
  tag). Photo and quote inclusion is changed on each library item; the server settings contract
  does not expose separate Photos and Quotes toggles.
- **Quiet hours**: a switch, then "From" and "To" time inputs (`type="time"`). Helper:
  "No refreshes in this window; the frame keeps the last image."
- **Sources**: rows for Google Photos and AI notes. Each shows its state ("Not set up",
  "Connected", "Token expired") with a link to the setup section of the docs. Rows for
  features the server reports as unavailable are hidden, not broken.
- **Frame**: "Refresh now", "Clear frame" (confirm).
- **Status** (route `/settings/status`): version and git revision, uptime, last refresh
  time and duration, refreshes today, panel state, disk free, photo count, CPU temperature
  when available. Read-only, monospace for values, refreshed every 30 s while open.

## Cross-cutting states

| State | Treatment |
|---|---|
| Loading | Skeleton blocks with the final dimensions, never a spinner that shifts layout |
| Can't reach the Pi | A banner under the status line: "Can't reach the frame. Check you're on the home Wi-Fi." with Retry. Drafts in Write are kept in `localStorage` so nothing typed is lost |
| Panel wedged (server 503) | Red status "Not responding" and the server's message; actions that would update the panel are disabled with that reason |
| Validation error | Inline under the field, linked with `aria-describedby`; summary only when several fields fail |
| Success | Toast plus `aria-live` announcement; no silent success |
| Destructive action | Undo toast for single items; confirmation dialog for bulk and for "Clear frame" |

## Design tokens

Defined once on `:root`, overridden for dark mode under
`@media (prefers-color-scheme: dark)`. Components use tokens, never raw hex. Contrast
ratios were computed with the WCAG formula; all text pairs meet 4.5:1 and input borders
meet 3:1.

### Colour

| Token | Light | Dark | Use and contrast |
|---|---|---|---|
| `--paper` | `#F6F3EE` | `#161412` | Page background |
| `--surface` | `#FFFDF9` | `#211E1B` | Cards, sheets, inputs |
| `--ink` | `#1C1917` | `#F3EEE6` | Text, primary button fill. 15.8:1 on paper |
| `--ink-muted` | `#57534E` | `#A8A29E` | Secondary text. 6.9:1 light, 7.3:1 dark |
| `--rule` | `#D6D0C6` | `#4A433D` | Hairline separators (decorative only) |
| `--field-border` | `#857D74` | `#857D74` | Input and control borders. 3.7:1 light, 4.5:1 dark |
| `--accent` | `#B4472A` | `#F08A6C` | Focus ring, selected state, links. 4.9:1 light, 7.5:1 dark |
| `--on-accent` | `#FFFFFF` | `#161412` | Text on accent fills. 5.4:1 light, 7.5:1 dark |
| `--ok` | `#15803D` | `#4ADE80` | "Ready" status. 4.5:1 light |
| `--warn-fg` / `--warn-bg` | `#92400E` / `#FEF3C7` | `#FCD34D` / `#3A2A0A` | Warnings. 6.4:1 light, 9.6:1 dark |
| `--danger` | `#B91C1C` | `#F87171` | Errors, destructive buttons. 5.9:1 light, 6.6:1 dark |

The primary button is ink on paper (inverted in dark mode): the most e-paper-like control
in the app. The accent is for focus, selection, and links, not for large fills.

### Type

| Role | Font | Size / line height | Weight |
|---|---|---|---|
| Wordmark "love-box" | `ui-serif, "New York", Georgia, serif` (system) | 22 / 28 | 400 italic |
| Screen title | System sans | 22 / 28 | 600 |
| Section heading | System sans | 17 / 24 | 600 |
| Body, inputs | System sans | 16 / 24 | 400 |
| Helper, captions | System sans | 14 / 20 | 400 |
| Status values | `ui-monospace, "SF Mono", Menlo, monospace` | 14 / 20 | 400 |

System sans is `system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`. Sizes are in
`rem` so the browser's text-size setting scales them. Nothing below 14 px; inputs never
below 16 px.

### Space, shape, motion

- Spacing scale: 4, 8, 12, 16, 24, 32, 48 px. Page gutter 16 px on phones, 24 px from
  768 px.
- Radius: 12 px cards and sheets, 10 px buttons and inputs, 999 px chips and the status
  dot. The frame image itself is always square-cornered.
- Elevation: none. Cards are separated by `--rule` hairlines and `--surface` against
  `--paper`. Sheets and toasts get one soft shadow in light mode only.
- Touch targets: at least 44 × 44 px, with at least 8 px between adjacent targets.
- Focus: `outline: 2px solid var(--accent); outline-offset: 2px` on `:focus-visible`,
  never removed.
- Motion: colour and opacity transitions of 150 ms, sheets slide in over 200 ms and out
  over 150 ms. No animated width, height, or layout. Under `prefers-reduced-motion:
  reduce`, all transitions become instant and the refresh flash is skipped.

## What the UI needs from the server

For U-301 to settle in `docs/API.md`; listed here so the two units meet:

1. SPA fallback: any `GET` outside `/api/*`, `/healthz`, and static assets returns
   `index.html`. Hashed assets are served with `Cache-Control: public, max-age=31536000,
   immutable`; `index.html` with `no-cache`.
2. Bundled font files served at a stable URL (for example `/fonts/<id>.ttf`) with the font
   list (`/api/fonts`: id, display name, italic available) so the picker can show samples.
3. Preview of a draft card returns the PNG plus fit metadata: the chosen font size for
   Auto, and whether fixed-size text was truncated.
4. A current-frame image URL that changes when the frame changes (an ETag or a revision
   query parameter), and a state document with: what is showing and why, mode, next
   change time, last refresh time and duration, panel state (`ready`, `busy`, `wedged`),
   quiet-hours status, and capability flags for features not built yet (scheduler, Google
   Photos, AI notes).
5. Error bodies with a human-readable message the UI can show verbatim, plus a stable code.
6. Server-generated ids for every photo and quote.
7. Upload accepts one file per request so the UI can show per-file progress and errors.

## Accessibility checklist (pre-delivery)

- [ ] Text contrast at least 4.5:1, input borders and focus at least 3:1, in both modes.
- [ ] Every input has a visible label; placeholders are examples, never labels.
- [ ] Icon-only buttons have an accessible name; decorative icons are `aria-hidden`.
- [ ] Status is never conveyed by colour alone.
- [ ] Previews have alt text describing the card.
- [ ] Keyboard: every action reachable, focus order follows the visual order, sheets trap
      and restore focus, Escape closes them.
- [ ] `aria-live="polite"` region announces frame updates and upload results.
- [ ] Works at 200 % text size and at 320 px wide without horizontal scrolling.
- [ ] `prefers-reduced-motion` and `prefers-color-scheme` both honoured.
- [ ] Viewport meta includes `viewport-fit=cover`; zoom is never disabled.

## Recommendations not taken

The design-system generator's first suggestion for this product was a dark "real-time
operations" look: slate background, status green accent, Plus Jakarta Sans loaded from
Google Fonts. It was rejected on three grounds: a picture frame is not an operations
console, the e-paper itself is warm white and black, and loading fonts from a CDN breaks
the "works with the internet down" rule. Kept from the guidance: the Swiss/minimal style
(low decoration, clear hierarchy, single accent), the warm "ink on cream" palette family
for writing apps, the touch, form, feedback, and motion rules, and the Phosphor icon
family.
