#!/usr/bin/env bash
# deploy/backup.sh [HOST] [DEST]
#
# Mirrors the Pi's state directory /var/lib/lovebox/ (SQLite db, photo library) to DEST
# on the Mac. HOST defaults to pi@lovebox.local, DEST to ~/lovebox-backup. Run it before
# any re-image.
#
# DEST becomes an exact mirror: files deleted on the Pi are deleted in DEST (--delete).
# No sudo on the Pi: the SSH user must be in group lovebox, once, on the Pi:
#   sudo usermod -aG lovebox pi     (then log out and back in)
set -euo pipefail

host="${1:-pi@lovebox.local}"
dest="${2:-$HOME/lovebox-backup}"

# Refuse destinations where --delete could wipe something that is not a backup.
dest_abs="$(mkdir -p "$dest" && cd "$dest" && pwd -P)"
case "$dest_abs" in
  / | "$HOME" | "$(cd "$HOME" && pwd -P)") echo "backup: refusing DEST=$dest_abs" >&2; exit 1 ;;
esac

echo "backup: $host:/var/lib/lovebox/ -> $dest_abs/"
rsync -az --delete -e "ssh -o BatchMode=yes -o ConnectTimeout=10" \
  "$host:/var/lib/lovebox/" "$dest_abs/"
echo "backup: done ($(du -sh "$dest_abs" | cut -f1))"
