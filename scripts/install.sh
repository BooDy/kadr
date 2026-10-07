#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Kadr Media Server — Universal Linux Installer & In-Place Upgrader
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/BooDy/kadr/main/scripts/install.sh | sudo bash
#
# Environment variables:
#   KADR_VERSION    Target release tag (e.g. v0.1.1-alpha). Default: latest GitHub release.
#   KADR_DRY_RUN    Set to 1 to test architecture and version resolution without system changes.
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

echo -e "${BLUE}${BOLD}=== Kadr Media Server Installer ===${NC}"

# 1. Privilege check
if [ "${KADR_DRY_RUN:-0}" != "1" ] && [ "${EUID:-$(id -u)}" -ne 0 ]; then
    echo -e "${RED}Error: This installer must be run as root.${NC}" >&2
    echo "Please re-run with sudo: curl -fsSL https://raw.githubusercontent.com/BooDy/kadr/main/scripts/install.sh | sudo bash" >&2
    exit 1
fi

# 2. Detect CPU architecture
ARCH="$(uname -m)"
case "${ARCH}" in
    x86_64|amd64)
        TARGET="x86_64-unknown-linux-musl"
        ;;
    aarch64|arm64)
        TARGET="aarch64-unknown-linux-musl"
        ;;
    *)
        echo -e "${RED}Error: Unsupported architecture: ${ARCH}.${NC}" >&2
        echo "Kadr currently provides pre-compiled static musl binaries for x86_64 and aarch64 (ARM64)." >&2
        exit 1
        ;;
esac

echo -e "Detected architecture: ${GREEN}${ARCH}${NC} (${TARGET})"

# 3. Detect downloader (curl or wget)
DOWNLOADER=""
if command -v curl >/dev/null 2>&1; then
    DOWNLOADER="curl"
elif command -v wget >/dev/null 2>&1; then
    DOWNLOADER="wget"
else
    echo -e "${RED}Error: Neither curl nor wget is installed.${NC}" >&2
    echo "Please install curl or wget and run the installer again." >&2
    exit 1
fi

fetch_url() {
    local url="$1"
    local dest="$2"
    if [ "${DOWNLOADER}" = "curl" ]; then
        curl -fsSL "${url}" -o "${dest}"
    else
        wget -qO "${dest}" "${url}"
    fi
}

fetch_string() {
    local url="$1"
    if [ "${DOWNLOADER}" = "curl" ]; then
        curl -fsSL "${url}" 2>/dev/null || true
    else
        wget -qO- "${url}" 2>/dev/null || true
    fi
}

# 4. Resolve target version
TAG="${KADR_VERSION:-}"
if [ -z "${TAG}" ]; then
    echo "Resolving latest Kadr release..."
    LATEST_JSON="$(fetch_string "https://api.github.com/repos/BooDy/kadr/releases/latest")"
    if [ -n "${LATEST_JSON}" ]; then
        TAG="$(echo "${LATEST_JSON}" | grep '"tag_name":' | head -n1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
    fi
    # Fallback to current release if API lookup fails or is rate-limited
    if [ -z "${TAG}" ]; then
        TAG="v0.1.1-alpha"
        echo -e "${YELLOW}Notice: Could not query GitHub API (rate limit or offline). Falling back to ${TAG}.${NC}"
    fi
fi

echo -e "Target version:        ${GREEN}${TAG}${NC}"

TARBALL_NAME="kadr-${TAG}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/BooDy/kadr/releases/download/${TAG}/${TARBALL_NAME}"

# Dry-run exit
if [ "${KADR_DRY_RUN:-0}" = "1" ]; then
    echo -e "${YELLOW}Dry-run mode active. No files were modified.${NC}"
    echo "Resolved URL: ${DOWNLOAD_URL}"
    exit 0
fi

# 5. Check if Kadr is already installed and running (In-Place Upgrade)
IS_UPGRADE=false
WAS_ACTIVE=false

if [ -f /usr/local/bin/kadr ] || [ -f /usr/bin/kadr ]; then
    IS_UPGRADE=true
    echo -e "${BLUE}Existing Kadr installation detected. Preparing in-place upgrade...${NC}"
fi

if command -v systemctl >/dev/null 2>&1; then
    if systemctl is-active --quiet kadr 2>/dev/null; then
        echo "Stopping active kadr service for upgrade..."
        systemctl stop kadr || true
        WAS_ACTIVE=true
    fi
fi

# 6. Create temporary working directory
TMP_DIR="$(mktemp -d /tmp/kadr-install-XXXXXX)"
cleanup() {
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT INT TERM

echo "Downloading ${TARBALL_NAME}..."
if ! fetch_url "${DOWNLOAD_URL}" "${TMP_DIR}/${TARBALL_NAME}"; then
    echo -e "${RED}Error: Failed to download release archive from:${NC}" >&2
    echo "  ${DOWNLOAD_URL}" >&2
    echo "Please verify that release ${TAG} exists on https://github.com/BooDy/kadr/releases." >&2
    exit 1
fi

echo "Extracting release bundle..."
tar -xzf "${TMP_DIR}/${TARBALL_NAME}" -C "${TMP_DIR}"

# Locate extracted contents
SRC_DIR="$(find "${TMP_DIR}" -maxdepth 2 -type f -name "kadr" -exec dirname {} \; | head -n1)"
if [ -z "${SRC_DIR}" ] || [ ! -f "${SRC_DIR}/kadr" ]; then
    echo -e "${RED}Error: Extracted archive did not contain kadr binary.${NC}" >&2
    exit 1
fi

# 7. Create system user and group (if not present)
if ! id -u kadr >/dev/null 2>&1; then
    echo "Creating system user and group 'kadr'..."
    useradd --system --no-create-home --user-group --shell /usr/sbin/nologin kadr 2>/dev/null || \
        adduser -S -D -H -s /sbin/nologin kadr 2>/dev/null || \
        useradd -r -s /bin/false kadr
else
    echo "System user 'kadr' exists."
fi

# 8. Install executable binary and symlinks
echo "Installing binary to /usr/local/bin/kadr..."
install -m 0755 "${SRC_DIR}/kadr" /usr/local/bin/kadr
ln -sf /usr/local/bin/kadr /usr/bin/kadr
ln -sf /usr/local/bin/kadr /usr/local/bin/kadr-server
ln -sf /usr/local/bin/kadr /usr/bin/kadr-server

# 9. Install web client SPA distribution
echo "Installing web assets to /usr/share/kadr/web..."
mkdir -p /usr/share/kadr/web
if [ -d "${SRC_DIR}/web" ]; then
    cp -r "${SRC_DIR}/web"/* /usr/share/kadr/web/
    chmod -R a+rX /usr/share/kadr/web
fi

# 10. Configure directories (data & configuration)
echo "Configuring data and configuration paths..."
mkdir -p /var/lib/kadr /var/lib/kadr/media /etc/kadr
chown -R kadr:kadr /var/lib/kadr
chmod 0750 /var/lib/kadr

# 11. Protect and initialize configuration files
if [ ! -f /etc/kadr/kadr.env ]; then
    if [ -f "${SRC_DIR}/kadr.env.example" ]; then
        cp "${SRC_DIR}/kadr.env.example" /etc/kadr/kadr.env
    elif [ -f "${SRC_DIR}/kadr.env" ]; then
        cp "${SRC_DIR}/kadr.env" /etc/kadr/kadr.env
    fi
    chmod 0640 /etc/kadr/kadr.env 2>/dev/null || true
    chown root:kadr /etc/kadr/kadr.env 2>/dev/null || true
else
    echo "Existing /etc/kadr/kadr.env preserved."
fi

if [ ! -f /etc/kadr/kadr.toml ]; then
    if [ -f "${SRC_DIR}/kadr.toml.example" ]; then
        cp "${SRC_DIR}/kadr.toml.example" /etc/kadr/kadr.toml
    elif [ -f "${SRC_DIR}/kadr.toml" ]; then
        cp "${SRC_DIR}/kadr.toml" /etc/kadr/kadr.toml
    fi
    chmod 0640 /etc/kadr/kadr.toml 2>/dev/null || true
    chown root:kadr /etc/kadr/kadr.toml 2>/dev/null || true
else
    echo "Existing /etc/kadr/kadr.toml preserved."
fi

# 12. Systemd service setup
if [ -d /etc/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
    echo "Installing systemd service /etc/systemd/system/kadr.service..."
    if [ -f "${SRC_DIR}/kadr.service" ]; then
        cp "${SRC_DIR}/kadr.service" /etc/systemd/system/kadr.service
    fi
    systemctl daemon-reload

    if [ "${WAS_ACTIVE}" = "true" ]; then
        echo "Restarting Kadr service..."
        systemctl restart kadr
    elif [ "${IS_UPGRADE}" = "false" ]; then
        echo "Starting and enabling Kadr service on boot..."
        systemctl enable --now kadr || systemctl start kadr || true
    fi
    echo -e "${GREEN}Systemd service configured successfully.${NC}"
else
    echo -e "${YELLOW}Notice: systemd not detected. You can run Kadr manually via:${NC}"
    echo "  su -s /bin/sh kadr -c '/usr/local/bin/kadr --config /etc/kadr/kadr.toml'"
fi

echo ""
if [ "${IS_UPGRADE}" = "true" ]; then
    echo -e "${GREEN}${BOLD}=== Kadr successfully upgraded to ${TAG}! ===${NC}"
else
    echo -e "${GREEN}${BOLD}=== Kadr ${TAG} successfully installed! ===${NC}"
fi
echo "Default Web Interface: http://localhost:8492"
echo "Data Directory:        /var/lib/kadr"
echo "Config Directory:      /etc/kadr"
echo ""
echo "Useful commands:"
echo "  Check status:        systemctl status kadr"
echo "  View live logs:      journalctl -u kadr -f"
echo "  Restart server:      systemctl restart kadr"
