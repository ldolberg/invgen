#!/bin/sh
# Build the Flatpak into the signed OSTree repo on the gh-pages branch and push it.
# Served at https://ldolberg.github.io/invgen/ (GitHub Pages, gh-pages branch).
#
# Signing key: a dedicated keyring, by default ~/.local/share/invgen/flatpak-gpg
# (override with FLATPAK_GPG_HOME / FLATPAK_GPG_KEY). Never commit it.
#
# CI (.github/workflows/release.yml) runs this too, with FLATPAK_BUILDER set to
# the host flatpak-builder and PUBLISH_PUSH=0 for dry runs.
set -eu

cd "$(dirname "$0")/../.."
APP_ID=io.github.ldolberg.invgen
MANIFEST=packaging/flatpak/$APP_ID.yml
SITE_URL=https://ldolberg.github.io/invgen
PAGES=build-pages
GPG_HOME=${FLATPAK_GPG_HOME:-$HOME/.local/share/invgen/flatpak-gpg}
GPG_KEY=${FLATPAK_GPG_KEY:-$(GNUPGHOME=$GPG_HOME gpg --list-secret-keys --with-colons | awk -F: '/^fpr/{print $10; exit}')}
BUILDER=${FLATPAK_BUILDER:-"flatpak run --env=FLATPAK_USER_DIR=$HOME/.local/share/flatpak --command=flatpak-builder org.flatpak.Builder --disable-rofiles-fuse"}

[ -n "$GPG_KEY" ] || { echo "no signing key in $GPG_HOME" >&2; exit 1; }

# gh-pages worktree (created on first run).
if [ ! -d "$PAGES" ]; then
    git fetch -q origin gh-pages 2>/dev/null || true
    if git rev-parse -q --verify origin/gh-pages >/dev/null; then
        git worktree add -q "$PAGES" gh-pages 2>/dev/null || git worktree add -q -b gh-pages "$PAGES" origin/gh-pages
    else
        git worktree add -q --orphan -b gh-pages "$PAGES"
    fi
fi
git -C "$PAGES" pull -q --ff-only origin gh-pages 2>/dev/null || true

$BUILDER --force-clean --default-branch=stable \
    --repo="$PAGES/repo" --gpg-sign="$GPG_KEY" --gpg-homedir="$GPG_HOME" \
    build-flatpak "$MANIFEST"
flatpak build-update-repo --generate-static-deltas --prune --prune-depth=5 \
    --title="invgen" --default-branch=stable \
    --gpg-sign="$GPG_KEY" --gpg-homedir="$GPG_HOME" "$PAGES/repo"

KEY_B64=$(GNUPGHOME=$GPG_HOME gpg --export "$GPG_KEY" | base64 -w0)
GNUPGHOME=$GPG_HOME gpg --armor --export "$GPG_KEY" > "$PAGES/invgen.gpg"

cat > "$PAGES/invgen.flatpakrepo" <<EOF
[Flatpak Repo]
Title=invgen
Url=$SITE_URL/repo/
Homepage=https://github.com/ldolberg/invgen
Comment=Markdown invoices to clean PDFs
Icon=$SITE_URL/icon.svg
GPGKey=$KEY_B64
EOF

cat > "$PAGES/$APP_ID.flatpakref" <<EOF
[Flatpak Ref]
Name=$APP_ID
Branch=stable
Title=invgen
Url=$SITE_URL/repo/
SuggestRemoteName=invgen
Homepage=https://github.com/ldolberg/invgen
Icon=$SITE_URL/icon.svg
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
IsRuntime=false
GPGKey=$KEY_B64
EOF

cp packaging/$APP_ID.svg "$PAGES/icon.svg"
cp packaging/flatpak/index.html "$PAGES/index.html"
touch "$PAGES/.nojekyll"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
git -C "$PAGES" add -A
git -C "$PAGES" commit -q -m "Flatpak repo: invgen $VERSION" || echo "nothing to commit"
if [ "${PUBLISH_PUSH:-1}" = 1 ]; then
    git -C "$PAGES" push -q origin gh-pages
    echo "published $VERSION to $SITE_URL"
else
    echo "built $VERSION into $PAGES (not pushed)"
fi
