#!/bin/sh
# Install a prebuilt invgen release into ~/.local (no root, no Rust needed).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
prefix=${PREFIX:-$HOME/.local}
install -Dm755 "$here/invgen" "$prefix/bin/invgen"
install -Dm644 "$here/io.github.ldolberg.invgen.desktop" "$prefix/share/applications/io.github.ldolberg.invgen.desktop"
install -Dm644 "$here/io.github.ldolberg.invgen.svg" "$prefix/share/icons/hicolor/scalable/apps/io.github.ldolberg.invgen.svg"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$prefix/share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true
echo "installed $prefix/bin/invgen (make sure $prefix/bin is on your PATH)"
echo "next: invgen config   # create ~/.config/invgen/config.yaml"
