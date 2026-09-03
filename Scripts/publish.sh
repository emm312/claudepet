#!/bin/bash
# Build ClaudePet.app and publish a GitHub Release asset that the app's
# in-app updater reads. Requires the GitHub CLI (`gh auth login` done once).
#
#   ./Scripts/publish.sh
#
# Auto-upload alternative: push a tag `vX.Y.Z` and .github/workflows/release.yml
# builds + uploads the same two assets (plus the Windows ones) on GitHub runners.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPO="emm312/claudepet"
VERSION="$(cat VERSION)"
TAG="v$VERSION"
echo "Publishing $TAG to $REPO"

"$ROOT_DIR/Scripts/bundle.sh"

ZIP="$ROOT_DIR/ClaudePet-mac.zip"
rm -f "$ZIP"
ditto -c -k --sequesterRsrc --keepParent "$ROOT_DIR/ClaudePet.app" "$ZIP"

# Checksum the zip - the updater verifies this before staging it. Asset names
# MUST stay `ClaudePet-mac.zip` / `ClaudePet-mac.zip.sha256` - Updater.swift
# looks them up by name.
shasum -a 256 "$ZIP" | awk '{print $1}' > "$ZIP.sha256"

if gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
    gh release upload "$TAG" "$ZIP" "$ZIP.sha256" --repo "$REPO" --clobber
else
    gh release create "$TAG" "$ZIP" "$ZIP.sha256" \
        --repo "$REPO" --target main --title "$TAG" \
        --notes "ClaudePet $TAG"
fi
echo "Done. Running $VERSION clients will update on their next check."
