SHELL := /bin/bash

VERSION      := $(shell node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null)
DIST         := dist
LINUX_BIN    := src-tauri/target/release/starlit-foss
WIN_DIR      := src-tauri/target/x86_64-pc-windows-gnu/release
WIN_EXE      := $(WIN_DIR)/starlit-foss.exe
PORTABLE     := $(DIST)/StarlitFOSS-$(VERSION)-linux-x86_64
INCLUDE_MEDIA ?= 1

.DEFAULT_GOAL := help
.PHONY: help deps media dev build check windows msi windows-msi linux-package clean distclean tui

help:
	@echo "Starlit FOSS build targets"
	@echo
	@echo "  make deps             install npm dependencies"
	@echo "  make media            download and verify the media packs"
	@echo "  make dev              run in development mode"
	@echo "  make build            bundle with tauri (all default targets)"
	@echo "  make check            cargo check and JS syntax check"
	@echo "  make windows          cross-compile the Windows exe (mingw-w64)"
	@echo "  make msi              package dist/StarlitFOSS-$(VERSION)-x64.msi with wixl"
	@echo "  make windows-msi      make windows + make msi"
	@echo "  make linux-package    build and assemble the portable Linux tarball"
	@echo "  make tui              open the TUI maintenance tool"
	@echo "  make clean            remove build output and dist artifacts"
	@echo
	@echo "Variables:"
	@echo "  INCLUDE_MEDIA=0       build a slim MSI without the media packs"
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

msi:
	INCLUDE_MEDIA=$(INCLUDE_MEDIA) ./scripts/build-msi.sh

windows-msi: windows msi

linux-package: 
	cd src-tauri && cargo build --release
	@test -x $(LINUX_BIN) || { echo "missing $(LINUX_BIN)" >&2; exit 1; }
	rm -rf $(PORTABLE)
	mkdir -p $(PORTABLE)/bin $(PORTABLE)/resources $(DIST)
	cp $(LINUX_BIN) $(PORTABLE)/bin/starlit-foss
	cp -r resources/template $(PORTABLE)/resources/template
	cp -r resources/media $(PORTABLE)/resources/media
	cp LICENSE README.md README.zh-CN.md README.zh-TW.md $(PORTABLE)/
	printf '%s\n' '#!/bin/sh' \
	  'DIR="$$(cd "$$(dirname "$$0")" && pwd)"' \
	  'export STARLIT_RESOURCE_ROOT="$$DIR/resources"' \
	  'exec "$$DIR/bin/starlit-foss" "$$@"' > $(PORTABLE)/starlit-foss
	chmod +x $(PORTABLE)/starlit-foss $(PORTABLE)/bin/starlit-foss
	tar -C $(DIST) -czf $(DIST)/StarlitFOSS-$(VERSION)-linux-x86_64.tar.gz StarlitFOSS-$(VERSION)-linux-x86_64
	sha256sum $(DIST)/StarlitFOSS-$(VERSION)-linux-x86_64.tar.gz > $(DIST)/StarlitFOSS-$(VERSION)-linux-x86_64.tar.gz.sha256
	@ls -lh $(DIST)/StarlitFOSS-$(VERSION)-linux-x86_64.tar.gz

tui:
	./tools/tui.sh

clean:
	cd src-tauri && cargo clean
	rm -rf $(DIST)/StarlitFOSS-$(VERSION)-linux-x86_64 $(DIST)/*.msi $(DIST)/*.tar.gz $(DIST)/*.sha256

distclean: clean
	rm -rf node_modules
