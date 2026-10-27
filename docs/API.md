# lovebox local API

The service binds to `0.0.0.0:8080` on a development machine and to `0.0.0.0:80` on the Pi when provisioned. It has no authentication and is intended for a trusted home LAN only. Do not forward the port to the public internet. `/api` allows cross-origin requests from any origin; browsers can read the preview metadata headers listed below.

All JSON is UTF-8. Timestamps are RFC 3339 UTC strings. Errors use one stable envelope:

```json
{"error":{"code":"not_found","message":"Photo '01J...' was not found."}}
```

The `message` is suitable for display. Common codes are `bad_request`, `validation_error`, `not_found`, `too_large`, `panel_unavailable`, `storage_error`, and `internal_error`. Malformed cards, invalid fields, unknown fonts, and renderer validation errors return 400; bodies above the upload limit return 413; missing IDs return 404; a wedged display returns 503; unexpected storage or rendering failures return 500. A failed request does not persist a mutation or advance the current-frame revision. If the panel updates successfully but writing the new state document then fails, `POST /api/show` returns 500 with an explicit partial-outcome message; the saved card, PNG, and revision remain unchanged, and the physical panel may differ until a later successful update.

## Cards and rendering

The API card is a JSON DTO. It is intentionally separate from the renderer's Rust `Card`; clients never submit image bytes as part of a card. Photo render controls are saved on the photo record and selected by ID.

```json
{"type":"message","text":"Good morning, love.","size":48,"font":"eb-garamond"}
```

`size` is one of `30`, `48`, `64`, or `96` pt. `font` is a stable font ID from `GET /api/fonts`; it may be omitted to use the saved default font. Message text is limited to 3,000 characters. Fixed-size messages keep the renderer's baseline layout and truncate overflow with `...`.

```json
{"type":"quote","text":"Hold fast to dreams.","attribution":"A reader","font":"eb-garamond","align":"center"}
```

`attribution` is optional, `font` defaults to the saved default font, and `align` is `left` or `center` (default `left`). Quotes are auto-fit from 14 to 120 px with the renderer's 40 px margins. The normalized text limit is 2,000 characters; text that cannot fit at the minimum size is rejected with the renderer's message.

```json
{"type":"photo","id":"01J9R6J5W4HF0E2PQS0A1B2C3D"}
```

The photo must exist. Its saved focal point, dither, and enhancement settings are used.

```json
{"type":"clear"}
```

A message whose text is exactly `clear` is also treated as a clear card.

## State and display

### `GET /healthz`

Returns operational information. Returns 503 when `panel.state` is `wedged`; the body still contains the health object and panel error.

```json
{
  "version":"0.1.0","git_sha":"a1faa76","uptime_s":120,
  "panel":{"name":"png","state":"ready","last_show_at":"2026-10-22T10:00:00Z","last_error":null,"shows_total":1,"last_refresh_duration_ms":14},
  "data_dir":"./data","disk_free_bytes":1234567890,"photos":2,"state_version":1,
  "cpu_temperature_c":null,"refresh_count_today":1,"quote_max_chars":2000
}
```

`cpu_temperature_c` is `null` when the host does not expose a temperature sensor. `refresh_count_today` is a best-effort local-day counter.

### `GET /api/state`

Returns what the frame shows, the mode, and when it next changes. `next_change_at` is `null` in pinned mode and when the library is empty; `queue` lists every photo and saved quote the slideshow and daily modes draw from.

```json
{
  "now_showing":{"type":"message","text":"Good morning, love.","size":48,"font":"eb-garamond"},
  "mode":"pinned","reason":"pinned","next_change_at":null,
  "current_frame_url":"/api/frame/current.png?rev=3","frame_revision":3,
  "settings":{"default_font":"eb-garamond","default_dither":"floyd_steinberg","enhance":true,"fast_refresh":true,"slideshow":{"interval_s":900,"order":"shuffle","set":"all"},"daily":{"time":"07:00"},"mode":"pinned"},
  "panel":{"name":"png","state":"ready","last_show_at":"2026-10-22T10:00:00Z","last_error":null,"shows_total":1,"last_refresh_duration_ms":14},
  "last_refresh_at":"2026-10-22T10:00:00Z","last_refresh_duration_ms":14,
  "quote_max_chars":2000,"message_max_chars":3000,
  "queue":[{"type":"photo","id":"01J..."},{"type":"quote","id":"01J..."}]
}
```

`reason` is `pinned` for the supported mode. State also includes `queue`, an array of saved photo and quote IDs. Every library item is in the slideshow and daily rotation; there is no per-item flag.

### `POST /api/show`

Renders, displays, then records the card as `now_showing` and adds one history entry. Request body is one Card as above. Returns 200 only after the panel operation succeeds:

```json
{"shown_at":"2026-10-22T10:00:00Z","frame_revision":3,"last_refresh_duration_ms":14,"paused":false}
```

A card shown here is pinned: if the frame was in slideshow or daily mode, the mode becomes `pinned` and `paused` is `true`, so a timed change does not replace it. Re-showing the card already on the frame (Refresh now) leaves the mode alone.

A missing photo returns 404. Invalid input returns 400. A wedged or unavailable panel returns 503 and leaves `now_showing` and the frame revision unchanged. If panel I/O succeeds but the following atomic state write fails, the request returns 500 and says the physical panel may differ from saved state; the frame endpoint and revision continue to represent the last persisted display state.

### `POST /api/clear`

Clears the display and records a clear card. No request body is required. Response and pinning are the same as `POST /api/show`.

### `GET /api/frame/current.png`

Returns the actual PNG from the most recent successful panel update, `image/png`, with an `ETag` containing the frame revision. The state response's `current_frame_url` includes the revision as a cache-busting query value. Before the first successful display the endpoint returns 404 (`no_frame`). A failed panel operation does not replace this image or its ETag.

### `POST /api/preview`

Takes one Card and returns the rendered 800×480 PNG without calling the panel or changing state. Response content type is `image/png`. Headers:

- `X-Lovebox-Fit-Size-Px`: chosen quote font size; omitted for non-quote cards.
- `X-Lovebox-Truncated`: `true` or `false` for fixed-size messages; omitted for other cards.
- `Access-Control-Expose-Headers`: exposes both fit headers and `ETag` to browser clients.

The same validation and render errors as `POST /api/show` apply. A photo ID must exist.

### `GET /api/fonts`

Returns bundled font metadata, including the stable file URL and italic availability. All font bytes are local; no CDN is used.

```json
[{"id":"eb-garamond","display_name":"EB Garamond","kind":"serif","italic_available":true,"file_url":"/fonts/eb-garamond.ttf"}]
```

### `GET /fonts/{id}.ttf`

Returns the bundled TrueType font file with `font/ttf`. IDs are those returned by `/api/fonts`. Unknown IDs return 404.

## Photos

`PhotoMeta` has this shape:

```json
{
  "id":"01J9R6J5W4HF0E2PQS0A1B2C3D","filename_original":"beach.jpg","caption":"Summer by the sea",
  "taken_at":null,"added_at":"2026-10-22T10:00:00Z","width":1600,"height":960,
  "source_id":null,"focal":{"x":0.5,"y":0.5},"dither":"floyd_steinberg","enhance":true,
  "shown_count":0,"last_shown_at":null,"tags":[]
}
```

`width` and `height` describe the server's bounded copy, at most 1600×960. `taken_at` comes from EXIF `DateTimeOriginal` when present. IDs and file paths are generated by the server; filenames are metadata only and are never used as paths.

### `GET /api/photos?offset=0&limit=50`

Returns newest first:

```json
{"items":[{"id":"01J...","filename_original":"beach.jpg","caption":null,"taken_at":null,"added_at":"2026-10-22T10:00:00Z","width":1600,"height":960,"source_id":null,"focal":null,"dither":"floyd_steinberg","enhance":true,"shown_count":0,"last_shown_at":null,"tags":[]}],"total":1}
```

`offset` defaults to 0 and `limit` to 50 (maximum 200). Invalid pagination returns 400.

### `POST /api/photos`

Accepts one `multipart/form-data` file per request. The request body is capped at 64 MiB. The required part is `file`; optional text parts are `caption`, `focal_x`, `focal_y`, `dither` (`floyd_steinberg` or `atkinson`), `enhance` (`true` or `false`), and `source_id`. If omitted, `dither` and `enhance` use the saved settings defaults. The server serializes image ingest so uploads wait before accumulating their multipart file bytes.

```sh
curl -F 'file=@beach.jpg' -F 'caption=Summer by the sea' http://lovebox.local/api/photos
```

Returns 201 with `PhotoMeta`. Unsupported or malformed images return 400. An image that exceeds decode limits returns 413. A body above 64 MiB returns 413. The original filename is retained only as metadata.

### `GET /api/photos/{id}`

Returns one `PhotoMeta`, or 404 if the ID is unknown or deleted.

### `GET /api/photos/{id}/thumb.jpg`

Returns the bounded colour copy as JPEG (`image/jpeg`), sized no larger than 400 px wide. Missing IDs return 404.

### `GET /api/photos/{id}/copy.jpg`

Returns the bounded colour copy as JPEG (`image/jpeg`), no larger than 1600×960. Missing IDs return 404.

### `GET /api/photos/{id}/frame.png`

Returns the dithered 800×480 preview for the photo with its saved render controls (`image/png`). Missing IDs return 404.

### `PATCH /api/photos/{id}`

Updates any supplied subset of `caption`, `focal`, `zoom`, `dither`, or `enhance`. Omit a field to leave it unchanged; send `null` for `caption` or `focal` to clear it. `focal` can also be `{ "x":0.5,"y":0.5 }` with each coordinate in 0..=1. Dither is `floyd_steinberg` or `atkinson`.

```json
{"focal":{"x":0.7,"y":0.4},"dither":"atkinson"}
```

Returns the updated `PhotoMeta`; re-renders `frame.png` before committing metadata. Invalid settings return 400, missing IDs return 404, and failed rendering or file writes leave stored metadata unchanged.

### `DELETE /api/photos/{id}`

Deletes the metadata and files; returns 204. If that photo is currently displayed, the panel and current frame stay as they are, while the library item is removed. Missing IDs return 404.

## Quotes

A saved quote is `{ "id":"01J...", "text":"...", "attribution":null, "font":"eb-garamond", "align":"left", "added_at":"...", "shown_count":0, "last_shown_at":null }`. Text uses the renderer's normalized 2,000-character limit; the renderer's validation message is returned verbatim.

On `PATCH /api/quotes/{id}`, omit `attribution` to leave it unchanged, send a string to set it, or send `null` to clear it.

### `GET /api/quotes`

Returns `{ "items":[Quote], "total":1 }`, newest first.

### `POST /api/quotes`

Request:

```json
{"text":"Hold fast to dreams.","attribution":"A reader","font":"eb-garamond","align":"center"}
```

Returns 201 with the new quote, including its server-generated ID. Unknown fonts, empty quotes, and text over 2,000 characters return 400. A 2,001-character input receives the renderer's quote-limit message.

### `PATCH /api/quotes/{id}`

Updates any supplied subset of `text`, `attribution`, `font`, and `align`; validates and fits the quote before saving. Returns the updated quote. Invalid input returns 400; missing IDs return 404.

### `DELETE /api/quotes/{id}`

Deletes the quote and returns 204. Missing IDs return 404.

## Settings

### `GET /api/settings`

Returns the full settings document described below, plus `scheduler: "not implemented"`.

### `PUT /api/settings`

Replaces the full settings document. Fields:

```json
{
  "default_font":"eb-garamond","default_dither":"floyd_steinberg","enhance":true,
  "fast_refresh":true,
  "slideshow":{"interval_s":900,"order":"shuffle","set":"all"},
  "daily":{"time":"07:00"},
  "mode":"pinned"
}
```

`interval_s` must be at least 15 (default 900); `order` is `shuffle` or `upload`; `set` is `all` or `tag:<name>`; `mode` is `pinned`, `slideshow`, or `daily`. `fast_refresh` (default `true`, and `true` for state saved before it existed) picks the panel's shorter refresh waveform: fewer flashes, at some risk of faint ghosting. `daily.time` uses `HH:MM`. A `quiet_hours` object in older state or requests is ignored. The response contains the saved document and `scheduler: "daily_slideshow"`. Invalid settings, including an interval below 15, return 400 and do not alter state.

## Web app and unknown paths

The embedded UI serves `/` and falls back to `index.html` for browser routes such as `/library/photos/01J...`. `index.html` is sent with `Cache-Control: no-cache`; hashed assets use `public, max-age=31536000, immutable`. Missing static files return a real 404. Unknown `/api` and `/api/...` routes return the JSON error envelope with 404 and are never sent the SPA document.

## Limits and behavior

- One upload file per request; 64 MiB request body; only one image ingest at a time.
- Server photo decode budget is 160 MiB and bounded copies are at most 1600×960; the laptop `lovebox import` command performs local conversion before upload.
- Quote limit is 2,000 normalized characters; message limit is 3,000 characters.
- History holds the latest 500 successful display operations.
- Panel I/O is serialized on one display worker and times out. A timed-out worker is wedged for the lifetime of the process; further display operations return 503. Restart the systemd service to recover.
- CORS is enabled for `/api` and exposes preview-fit headers and `ETag`. There is no authentication because this service is for a trusted home LAN only.
