SHELL := /bin/bash

VERSION      := $(shell node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null)
DIST         := dist
LINUX_BIN    := src-tauri/target/release/starlit-foss
WIN_DIR      := src-tauri/target/x86_64-pc-windows-gnu/release
WIN_EXE      := $(WIN_DIR)/starlit-foss.exe
PORTABLE     := $(DIST)/StarlitFOSS-$(VERSION)-linux-x86_64
INCLUDE_MEDIA ?= 1

.DEFAULT_GOAL := help
.PHONY: help deps media dev build check windows msi windows-msi clean clean-package distclean tui

help:
	@echo "Starlit FOSS build targets"
	@echo
	@echo "  make deps             install npm dependencies"
	@echo "  make media            download and verify the media packs"
	@echo "  make dev              run in development mode"
	@echo "  make build            bundle with tauri (all default targets)"
	@echo "  make check            cargo check and JS syntax check"
	@echo "  make windows          cross-compile the Windows exe (mingw-w64)"
	@echo "  make pkg              build a FreeBSD .pkg (run on FreeBSD)"
	@echo "  make tar              build the portable Linux tarball"
	@echo "  make msi              build the Windows MSI with wixl"
	@echo "  make windows-msi      make windows + make msi"
	@echo "  make package          list packaging formats"
	@echo "  make tui              open the TUI maintenance tool"
	@echo "  make clean            remove build output"
	@echo "  make clean-package    remove packaged artifacts and stale staging dirs"
	@echo
	@echo "Variables:"
	@echo "  INCLUDE_MEDIA=0       skip media archives when packaging"
	@echo "  VERSION=$(VERSION)"

deps:
	npm install

media:
	./scripts/fetch-media.sh

dev: deps
	npm run dev

build: deps
	npm run build

check:
	cd src-tauri && cargo check
	node --check src/app.js
	node --check src/api.js

windows:
	@rustup target list --installed | grep -qx 'x86_64-pc-windows-gnu' || rustup target add x86_64-pc-windows-gnu
	@command -v x86_64-w64-mingw32-gcc >/dev/null || { echo "missing mingw-w64 (x86_64-w64-mingw32-gcc)" >&2; exit 1; }
	npx tauri build --no-bundle --target x86_64-pc-windows-gnu --ci
	@ls -lh $(WIN_EXE)

pkg:
	./packaging/package.sh freebsd

tar:
	./packaging/package.sh tar

msi:
	./packaging/package.sh msi

package:
	./packaging/package.sh list

windows-msi: windows msi

tui:
	./tools/tui.sh

clean:
	cd src-tauri && cargo clean
	rm -rf $(DIST)/StarlitFOSS-$(VERSION)-linux-x86_64

clean-package:
	rm -rf $(DIST)/*.pkg $(DIST)/*.msi $(DIST)/*.tar.gz $(DIST)/*.sha256 $(DIST)/starlit-msi.wxs
	rm -rf $${TMPDIR:-/tmp}/starlit-package.*

distclean: clean
	rm -rf node_modules
