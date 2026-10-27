#!/usr/bin/env bash
# deploy/check-pi.sh [--static-only] BINARY
#
# Proves BINARY is fit for the Raspberry Pi Zero W v1 (ARM1176JZF-S, ARMv6), on the Mac:
#   (a) file(1) says: ELF 32-bit LSB ... ARM, EABI5
#   (b) readelf -A says Tag_CPU_arch is v6, v6K, v6KZ or v6T2; v7 or later fails loudly
#   (c) it runs under Docker's qemu user emulation and prints `lovebox <version>`
# --static-only skips (c), for deploy.sh, so shipping does not need Docker running.
#
# (c) cannot prove ARMv6: Docker's qemu-arm emulates a newer CPU and will happily run
# ARMv7 instructions. (b) is the ARMv6 proof; the real test is running on the Pi.
#
# Needs: file (macOS), greadelf (`brew install binutils`) or readelf, docker with an arm
# binfmt emulator (colima on this Mac; see deploy/README.md).
set -euo pipefail

static_only=0
if [[ "${1:-}" == "--static-only" ]]; then
  static_only=1
  shift
fi
bin="${1:?usage: deploy/check-pi.sh [--static-only] BINARY}"
[[ -f "$bin" ]] || { echo "check-pi: FAIL: no such file: $bin (run make build-pi)" >&2; exit 1; }

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$repo_root/lovebox/Cargo.toml" | head -n 1)"
fail() { echo "check-pi: FAIL: $*" >&2; exit 1; }

echo "check-pi: binary $bin ($(wc -c < "$bin" | tr -d ' ') bytes)"

# (a) ELF class, endianness, machine, ABI.
file_out="$(file -b "$bin")"
echo "check-pi: (a) file: $file_out"
[[ "$file_out" =~ ^ELF\ 32-bit\ LSB.*ARM,\ EABI5 ]] || fail "(a) expected 'ELF 32-bit LSB ... ARM, EABI5'"
echo "check-pi: (a) PASS"

# (b) ARM build attributes: the CPU architecture the linker recorded for the whole image.
readelf="${READELF:-}"
if [[ -z "$readelf" ]]; then
  for candidate in greadelf readelf /opt/homebrew/opt/binutils/bin/greadelf; do
    if command -v "$candidate" > /dev/null 2>&1; then readelf="$candidate"; break; fi
  done
fi
[[ -n "$readelf" ]] || fail "(b) no readelf; run: brew install binutils"
attrs="$("$readelf" -A "$bin")"
cpu_line="$(grep -E '^[[:space:]]*Tag_CPU_arch:' <<< "$attrs" | head -n 1 | sed 's/^[[:space:]]*//')"
echo "check-pi: (b) $readelf -A: ${cpu_line:-<no Tag_CPU_arch>}"
grep -E '^[[:space:]]*Tag_(CPU_name|FP_arch|ABI_VFP_args):' <<< "$attrs" | sed 's/^[[:space:]]*/check-pi: (b)   /' || true
arch="${cpu_line#Tag_CPU_arch: }"
case "$arch" in
  v6 | v6K | v6KZ | v6T2) echo "check-pi: (b) PASS ($arch is ARMv6)" ;;
  "") fail "(b) no Tag_CPU_arch attribute; cannot prove ARMv6" ;;
  *) fail "(b) Tag_CPU_arch is '$arch', not ARMv6. This binary would crash on a Pi Zero W." ;;
esac

# Informational: the highest glibc symbol version the binary needs (Bookworm has 2.36).
glibc_max="$("$readelf" -W --dyn-syms "$bin" 2> /dev/null | grep -o 'GLIBC_[0-9.]*' | sort -t. -k1,1 -k2,2n -k3,3n -u | tail -n 1 || true)"
echo "check-pi: info: highest glibc symbol version required: ${glibc_max:-none (static)}"

if (( static_only )); then
  echo "check-pi: (c) skipped (--static-only)"
  exit 0
fi

# (c) Run it under emulation. glibc build -> Debian Bookworm (glibc 2.36); static -> Alpine.
if [[ "$file_out" == *"statically linked"* ]]; then
  platform="linux/arm/v6"; image="arm32v6/alpine:3.20"
else
  platform="linux/arm/v7"; image="arm32v7/debian:bookworm-slim"
fi
command -v docker > /dev/null 2>&1 || fail "(c) docker not found"
docker info > /dev/null 2>&1 || fail "(c) docker daemon not reachable; start it: colima start"
bindir="$(cd "$(dirname "$bin")" && pwd)"
name="$(basename "$bin")"
if ! run_out="$(docker run --rm --platform "$platform" -v "$bindir:/w:ro" "$image" "/w/$name" --version 2>&1)"; then
  if [[ "$run_out" == *"exec format error"* ]]; then
    echo "check-pi: hint: the Docker VM has no ARM emulator. Register one (lasts until the VM restarts):" >&2
    echo "check-pi: hint:   docker run --privileged --rm tonistiigi/binfmt --install arm" >&2
  fi
  fail "(c) emulated run failed: $run_out"
fi
echo "check-pi: (c) docker run --platform $platform $image /w/$name --version -> $run_out"
[[ "$run_out" == "lovebox $version" ]] || fail "(c) expected 'lovebox $version'"
echo "check-pi: (c) PASS (qemu does not enforce an ARM11 CPU; (b) is the ARMv6 proof)"
echo "check-pi: ALL PASS"
