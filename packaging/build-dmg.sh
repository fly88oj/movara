#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Build a .dmg installer image for macOS (run on macOS).
# Usage: packaging/build-dmg.sh [target-triple]   (default: host triple)
set -euo pipefail

TRIPLE="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
STAGE="target/${TRIPLE}/release/dmg-stage"
OUT="target/${TRIPLE}/release/movara-${VERSION}-${TRIPLE}.dmg"
VOLNAME="movara-${VERSION}"

command -v hdiutil >/dev/null || { echo "hdiutil not found (macOS only)"; exit 1; }

rm -rf "${STAGE}"
mkdir -p "${STAGE}/bin" "${STAGE}/share/doc/movara"
cp "target/${TRIPLE}/release/movara" "${STAGE}/bin/"
cp README.md LICENSE-APACHE LICENSE-MIT "${STAGE}/share/doc/movara/"
cat > "${STAGE}/INSTALL.txt" <<EOF
movara ${VERSION} (${TRIPLE})

Install by copying the binary to a directory on your PATH, e.g.:

  cp bin/movara /usr/local/bin/

See share/doc/movara/README.md for usage.
EOF

rm -f "${OUT}"
hdiutil create -volname "${VOLNAME}" -srcfolder "${STAGE}" -ov -format UDZO "${OUT}"
echo "built ${OUT}"
