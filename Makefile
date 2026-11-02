# invgen: build, test and install for the current user (no root needed).

PREFIX   ?= $(HOME)/.local
BINDIR   ?= $(PREFIX)/bin
APPDIR   ?= $(PREFIX)/share/applications
ICONDIR  ?= $(PREFIX)/share/icons/hicolor/scalable/apps
APP_ID   := io.github.invgen
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
