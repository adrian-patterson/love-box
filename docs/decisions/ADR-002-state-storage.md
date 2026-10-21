# ADR-002: State lives in a JSON document, not SQLite

Date: 2026-10-21 · Status: accepted (manager decision within ADR-001's standing authority)

## Context

docs/PLAN.md originally named SQLite for settings, the card queue, photo metadata, quotes,
and show history. On the Pi Zero W the binary is cross-compiled for ARMv6 through
`cargo-zigbuild`; `rusqlite`'s bundled SQLite is ~250k lines of C compiled by `zig cc`
for every build and is the only C dependency the project would have. The data is small:
one household's settings, a few thousand photo records, a few hundred quotes, a capped
history.

## Decision

- All mutable state is one serde-serialised document, `/var/lib/lovebox/state.json`,
  held in memory behind a lock and written atomically (write temp file, fsync, rename) on
  every change. Schema carries a `version` field; migrations are code in `store.rs`.
- Photos live as files under `/var/lib/lovebox/photos/<id>/` (bounded copy + rendered
  frame + sidecar metadata inside the state document, not beside the files).
- Show history is capped (500 entries) inside the document.
- `backup.sh` therefore backs up everything by copying one directory.

## Consequences

- Zero C dependencies; `make check-pi` stays trivially green.
- No concurrent writers by design (one process); the lock serialises the API.
- If the document ever grows past a few megabytes, revisit; that is far beyond this frame.
