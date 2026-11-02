# invgen: build, test and install for the current user (no root needed).

PREFIX   ?= $(HOME)/.local
BINDIR   ?= $(PREFIX)/bin
APPDIR   ?= $(PREFIX)/share/applications
ICONDIR  ?= $(PREFIX)/share/icons/hicolor/scalable/apps
APP_ID   := io.github.ldolberg.invgen
CARGO    ?= cargo

.PHONY: all build test install uninstall fmt lint clean

all: build

build:
	$(CARGO) build --release --locked

test:
	$(CARGO) test --locked

install: build
	install -Dm755 target/release/invgen $(DESTDIR)$(BINDIR)/invgen
	install -Dm644 packaging/$(APP_ID).desktop $(DESTDIR)$(APPDIR)/$(APP_ID).desktop
	install -Dm644 packaging/$(APP_ID).svg $(DESTDIR)$(ICONDIR)/$(APP_ID).svg
	-command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q $(DESTDIR)$(APPDIR) || true
	-command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t $(DESTDIR)$(PREFIX)/share/icons/hicolor || true
	@echo "installed $(BINDIR)/invgen (make sure $(BINDIR) is on your PATH)"

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/invgen
	rm -f $(DESTDIR)$(APPDIR)/$(APP_ID).desktop
	rm -f $(DESTDIR)$(ICONDIR)/$(APP_ID).svg
	-command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q $(DESTDIR)$(APPDIR) || true

fmt:
	$(CARGO) fmt --all

lint:
	$(CARGO) clippy --all-targets --locked -- -D warnings

clean:
	$(CARGO) clean

# Release tarball. Home paths are remapped so no local paths end up in the binary.
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
DIST := dist/invgen-v$(VERSION)-x86_64-linux
dist:
	RUSTFLAGS="--remap-path-prefix=$(HOME)=/build" cargo build --release --locked
	rm -rf $(DIST) && mkdir -p $(DIST)
	cp target/release/invgen packaging/install.sh packaging/$(APP_ID).desktop packaging/$(APP_ID).svg README.md LICENSE docs/FORMAT.md examples/config.yaml $(DIST)/
	cp -r examples/invoices $(DIST)/
	strip $(DIST)/invgen
	cd dist && tar czf invgen-v$(VERSION)-x86_64-linux.tar.gz invgen-v$(VERSION)-x86_64-linux && sha256sum invgen-v$(VERSION)-x86_64-linux.tar.gz > invgen-v$(VERSION)-x86_64-linux.tar.gz.sha256

.PHONY: dist

# Flatpak (needs: flatpak install --user flathub org.flatpak.Builder
#   org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08)
FLATPAK_MANIFEST := packaging/flatpak/$(APP_ID).yml
FLATPAK_BUILDER  := flatpak run --env=FLATPAK_USER_DIR=$(HOME)/.local/share/flatpak --command=flatpak-builder org.flatpak.Builder --disable-rofiles-fuse

# Regenerate offline cargo sources after Cargo.lock changes.
flatpak-sources:
	curl -fsSLo build-flatpak-gen.py https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
	uv run --no-project --with aiohttp --with tomlkit python build-flatpak-gen.py Cargo.lock -o packaging/flatpak/cargo-sources.json
	rm -f build-flatpak-gen.py

# Build and install for the current user.
flatpak:
	$(FLATPAK_BUILDER) --user --install --force-clean build-flatpak $(FLATPAK_MANIFEST)

# Single-file bundle for releases: dist/invgen-v$(VERSION).flatpak
flatpak-bundle:
	$(FLATPAK_BUILDER) --force-clean --repo=build-flatpak-repo build-flatpak $(FLATPAK_MANIFEST)
	mkdir -p dist
	flatpak build-bundle build-flatpak-repo dist/invgen-v$(VERSION).flatpak $(APP_ID) --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo

.PHONY: flatpak flatpak-bundle flatpak-sources

# Publish to the self-hosted Flatpak repo (gh-pages, https://ldolberg.github.io/invgen/).
flatpak-publish:
	packaging/flatpak/publish.sh

.PHONY: flatpak-publish
