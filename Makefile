# love-box developer entry points. Run from the repo root. Details: deploy/README.md.
#
#   make build-pi                 cross-compile lovebox for the Pi Zero W (ARMv6, glibc 2.36)
#   make check-pi                 build-pi, then prove ARMv6 + run it under Docker emulation
#   make deploy [HOST=pi@lovebox.local]    build, check, ship, restart (deploy/deploy.sh)
#   make rollback [HOST=...]      swap lovebox.prev back in and restart
#   make backup [HOST=...] [DEST=~/lovebox-backup]   rsync /var/lib/lovebox to the Mac
#   make test | make lint | make run    native cargo test / fmt+clippy / run on the Mac

SHELL := /bin/bash

CRATE     := lovebox
PI_TARGET := arm-unknown-linux-gnueabihf
# glibc floor for the cross build: Raspberry Pi OS Bookworm ships 2.36 (Trixie's 2.41 is
# backwards compatible). cargo-zigbuild reads it from the target suffix.
PI_GLIBC  := 2.36
PI_BIN    := $(CRATE)/target/$(PI_TARGET)/release/lovebox
HOST      ?= pi@lovebox.local
DEST      ?= $(HOME)/lovebox-backup

# rustup's cargo even when the shell profile has not been re-sourced.
export PATH := $(HOME)/.cargo/bin:$(PATH)
# zig for cargo-zigbuild comes from PyPI's `ziglang` in this venv (see deploy/README.md);
# without the venv, cargo-zigbuild falls back to a `zig` on PATH.
ZIG_VENV ?= $(HOME)/.local/share/ziglang-venv
ifneq ($(wildcard $(ZIG_VENV)/bin/python),)
export CARGO_ZIGBUILD_PYTHON_PATH ?= $(ZIG_VENV)/bin/python
endif

.PHONY: build-pi check-pi deploy rollback backup test lint run

build-pi:
	cd $(CRATE) && cargo zigbuild --release --target $(PI_TARGET).$(PI_GLIBC)
	@ls -l $(PI_BIN)

check-pi: build-pi
	deploy/check-pi.sh $(PI_BIN)

deploy:
	deploy/deploy.sh $(HOST)

rollback:
	deploy/deploy.sh --rollback $(HOST)

backup:
	deploy/backup.sh $(HOST) $(DEST)

test:
	cd $(CRATE) && cargo test

lint:
	cd $(CRATE) && cargo fmt --check && cargo clippy --all-targets -- -D warnings

run:
	cd $(CRATE) && cargo run -- serve
