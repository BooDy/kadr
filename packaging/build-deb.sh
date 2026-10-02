#!/usr/bin/env bash
set -euo pipefail

# Build a Debian (.deb) package for Kadr Media Server.
# Usage: ./packaging/build-deb.sh <binary_path> <web_dist_path> <version> <arch> <output_dir>

if [ "$#" -ne 5 ]; then
    echo "Usage: $0 <binary_path> <web_dist_path> <version> <arch> <output_dir>"
    echo "Example: $0 ./target/release/kadr-server ./web/dist 0.1.0 amd64 ./dist"
    exit 1
fi

BIN_PATH="$1"
WEB_PATH="$2"
VERSION="$3"
ARCH="$4"
OUTPUT_DIR="$5"

# Clean version (strip leading 'v' if present)
VERSION="${VERSION#v}"

echo "Building Debian package for Kadr v${VERSION} (${ARCH})..."

WORK_DIR=$(mktemp -d -t kadr-deb-XXXXXX)
trap 'rm -rf "${WORK_DIR}"' EXIT

PKG_DIR="${WORK_DIR}/kadr_${VERSION}_${ARCH}"
mkdir -p "${PKG_DIR}/DEBIAN"
mkdir -p "${PKG_DIR}/usr/bin"
mkdir -p "${PKG_DIR}/usr/share/kadr/web"
mkdir -p "${PKG_DIR}/etc/kadr"
mkdir -p "${PKG_DIR}/lib/systemd/system"
mkdir -p "${PKG_DIR}/var/lib/kadr"

# Copy binary
install -m 0755 "${BIN_PATH}" "${PKG_DIR}/usr/bin/kadr"
ln -sf /usr/bin/kadr "${PKG_DIR}/usr/bin/kadr-server"

# Copy web assets
cp -r "${WEB_PATH}"/* "${PKG_DIR}/usr/share/kadr/web/"
find "${PKG_DIR}/usr/share/kadr/web" -type f -exec chmod 0644 {} +
find "${PKG_DIR}/usr/share/kadr/web" -type d -exec chmod 0755 {} +

# Copy configs & systemd unit
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cp "${SCRIPT_DIR}/systemd/kadr.service" "${PKG_DIR}/lib/systemd/system/kadr.service"
chmod 0644 "${PKG_DIR}/lib/systemd/system/kadr.service"

cp "${SCRIPT_DIR}/systemd/kadr.env.example" "${PKG_DIR}/etc/kadr/kadr.env"
cp "${SCRIPT_DIR}/systemd/kadr.toml.example" "${PKG_DIR}/etc/kadr/kadr.toml"
chmod 0640 "${PKG_DIR}/etc/kadr/"*

# Generate DEBIAN/control
INSTALLED_SIZE=$(du -sk "${PKG_DIR}" | cut -f1)

cat <<EOF > "${PKG_DIR}/DEBIAN/control"
Package: kadr
Version: ${VERSION}
Section: video
Priority: optional
Architecture: ${ARCH}
Installed-Size: ${INSTALLED_SIZE}
Maintainer: Kadr Contributors <https://github.com/BooDy/kadr>
Homepage: https://github.com/BooDy/kadr
Description: Minimalist, ultra-high-performance self-hosted media server
 Kadr is an embedded media server written in pure Rust with SQLite WAL
 storage, zero-copy HTTP 206 streaming, declarative layout hydration,
 on-the-fly WebVTT subtitle conversions, and a React cinema web app.
EOF

# Copy maintainer scripts
cp "${SCRIPT_DIR}/debian/postinst" "${PKG_DIR}/DEBIAN/postinst"
cp "${SCRIPT_DIR}/debian/prerm" "${PKG_DIR}/DEBIAN/prerm"
cp "${SCRIPT_DIR}/debian/postrm" "${PKG_DIR}/DEBIAN/postrm"
chmod 0755 "${PKG_DIR}/DEBIAN/postinst" "${PKG_DIR}/DEBIAN/prerm" "${PKG_DIR}/DEBIAN/postrm"

# Mark configuration files in conffiles
cat <<EOF > "${PKG_DIR}/DEBIAN/conffiles"
/etc/kadr/kadr.env
/etc/kadr/kadr.toml
EOF

# Build package
mkdir -p "${OUTPUT_DIR}"
dpkg-deb --build --root-owner-group "${PKG_DIR}" "${OUTPUT_DIR}/kadr_${VERSION}_${ARCH}.deb"

echo "Debian package created: ${OUTPUT_DIR}/kadr_${VERSION}_${ARCH}.deb"
