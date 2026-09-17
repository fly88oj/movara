#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Local packaging entry point.
#   Linux : tar.gz + .deb (cargo-deb) + .rpm (cargo-generate-rpm)
#   macOS : tar.gz + .dmg (packaging/build-dmg.sh)
# The GitHub Actions release workflow runs the same tools; use this script
# to reproduce packages locally.
set -euo pipefail

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"

cargo build --release
OUTDIR="target/package"
mkdir -p "${OUTDIR}"

# --- tarball / zip (all platforms) ---
if [[ "${TRIPLE}" == *windows* ]]; then
  7z a "target/package/movara-${VERSION}-${TRIPLE}.zip" \
     ./target/release/movara.exe \
     README.md LICENSE-APACHE LICENSE-MIT >/dev/null
else
  # stage the docs next to the binaries first: a chained second -C would
  # resolve inside the release dir and miss them
  cp README.md LICENSE-APACHE LICENSE-MIT target/release/
  tar -czf "${OUTDIR}/movara-${VERSION}-${TRIPLE}.tar.gz" \
      -C target/release movara README.md LICENSE-APACHE LICENSE-MIT
fi

case "${TRIPLE}" in
  *linux*)
    # --- .deb ---
    command -v cargo-deb >/dev/null || cargo install cargo-deb --locked
    cargo deb && cp target/debian/movara_${VERSION}-1_*.deb "${OUTDIR}/"
    # --- .rpm ---
    command -v cargo-generate-rpm >/dev/null \
      || cargo install cargo-generate-rpm --locked
    cargo generate-rpm && cp target/generate-rpm/movara-*.rpm "${OUTDIR}/"
    ;;
  *darwin*)
    packaging/build-dmg.sh
    cp "target/${TRIPLE}/release/movara-${VERSION}-${TRIPLE}.dmg" \
       "${OUTDIR}/" 2>/dev/null || true
    ;;
esac

ls -lh "${OUTDIR}"
