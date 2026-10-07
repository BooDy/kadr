#!/usr/bin/env bash
set -euo pipefail

# Kadr Media Server Linux Standalone Installer
# Installs binary, web assets, configuration, and systemd service.
# Supports both fresh installations and in-place upgrades.

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

echo -e "${BLUE}${BOLD}=== Installing Kadr Media Server ===${NC}"

if [ "${EUID:-$(id -u)}" -ne 0 ]; then
    echo -e "${RED}Error: This script must be run as root (use sudo).${NC}" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PARENT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Verify binary and web assets exist
if [ -f "${PARENT_DIR}/kadr" ]; then
    SRC_BIN="${PARENT_DIR}/kadr"
elif [ -f "${SCRIPT_DIR}/kadr" ]; then
    SRC_BIN="${SCRIPT_DIR}/kadr"
elif [ -f "${PARENT_DIR}/kadr-server" ]; then
    SRC_BIN="${PARENT_DIR}/kadr-server"
elif [ -f "${SCRIPT_DIR}/kadr-server" ]; then
    SRC_BIN="${SCRIPT_DIR}/kadr-server"
else
    echo -e "${RED}Error: kadr binary not found in distribution archive.${NC}" >&2
    exit 1
fi

SRC_WEB="${PARENT_DIR}/web"
if [ ! -d "${SRC_WEB}" ]; then
    SRC_WEB="${SCRIPT_DIR}/web"
fi

if [ ! -d "${SRC_WEB}" ]; then
    echo -e "${RED}Error: Web assets directory not found in distribution archive.${NC}" >&2
    exit 1
fi

# Detect active installation and pause service during binary replacement
WAS_ACTIVE=false
if command -v systemctl >/dev/null 2>&1; then
    if systemctl is-active --quiet kadr 2>/dev/null; then
        echo "Stopping active kadr service for upgrade..."
        systemctl stop kadr || true
        WAS_ACTIVE=true
    fi
fi

# 1. Create system user and group
if ! id -u kadr >/dev/null 2>&1; then
    echo "Creating system user 'kadr'..."
    useradd --system --no-create-home --user-group --shell /usr/sbin/nologin kadr 2>/dev/null || \
        adduser -S -D -H -s /sbin/nologin kadr 2>/dev/null || \
        useradd -r -s /bin/false kadr
else
    echo "System user 'kadr' already exists."
fi

# 2. Install executable
echo "Installing kadr binary to /usr/local/bin..."
install -m 0755 "${SRC_BIN}" /usr/local/bin/kadr
ln -sf /usr/local/bin/kadr /usr/local/bin/kadr-server
ln -sf /usr/local/bin/kadr /usr/bin/kadr
ln -sf /usr/local/bin/kadr /usr/bin/kadr-server

# 3. Install web client assets
echo "Installing web assets to /usr/share/kadr/web..."
mkdir -p /usr/share/kadr/web
cp -r "${SRC_WEB}"/* /usr/share/kadr/web/
chmod -R a+rX /usr/share/kadr/web

# 4. Setup directories
echo "Configuring data and config directories..."
mkdir -p /var/lib/kadr /var/lib/kadr/media /etc/kadr
chown -R kadr:kadr /var/lib/kadr
chmod 0750 /var/lib/kadr

# 5. Setup configuration files if not present
if [ ! -f /etc/kadr/kadr.env ]; then
    if [ -f "${PARENT_DIR}/kadr.env.example" ]; then
        cp "${PARENT_DIR}/kadr.env.example" /etc/kadr/kadr.env
    elif [ -f "${SCRIPT_DIR}/../systemd/kadr.env.example" ]; then
        cp "${SCRIPT_DIR}/../systemd/kadr.env.example" /etc/kadr/kadr.env
    fi
    chmod 0640 /etc/kadr/kadr.env 2>/dev/null || true
    chown root:kadr /etc/kadr/kadr.env 2>/dev/null || true
else
    echo "Preserving existing /etc/kadr/kadr.env."
fi

if [ ! -f /etc/kadr/kadr.toml ]; then
    if [ -f "${PARENT_DIR}/kadr.toml.example" ]; then
        cp "${PARENT_DIR}/kadr.toml.example" /etc/kadr/kadr.toml
    elif [ -f "${SCRIPT_DIR}/../systemd/kadr.toml.example" ]; then
        cp "${SCRIPT_DIR}/../systemd/kadr.toml.example" /etc/kadr/kadr.toml
    fi
    chmod 0640 /etc/kadr/kadr.toml 2>/dev/null || true
    chown root:kadr /etc/kadr/kadr.toml 2>/dev/null || true
else
    echo "Preserving existing /etc/kadr/kadr.toml."
fi

# 6. Install systemd service
SERVICE_SRC=""
if [ -f "${PARENT_DIR}/kadr.service" ]; then
    SERVICE_SRC="${PARENT_DIR}/kadr.service"
elif [ -f "${SCRIPT_DIR}/../systemd/kadr.service" ]; then
    SERVICE_SRC="${SCRIPT_DIR}/../systemd/kadr.service"
fi

if [ -n "${SERVICE_SRC}" ] && [ -d /etc/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
    echo "Installing systemd service..."
    cp "${SERVICE_SRC}" /etc/systemd/system/kadr.service
    systemctl daemon-reload
    if [ "${WAS_ACTIVE}" = "true" ]; then
        echo "Restarting kadr service..."
        systemctl restart kadr
    fi
    echo -e "${GREEN}Service installed!${NC}"
    echo "To start Kadr now:       systemctl start kadr"
    echo "To enable on boot:       systemctl enable kadr"
    echo "To view live logs:       journalctl -u kadr -f"
fi

echo -e "${GREEN}${BOLD}=== Kadr installed successfully! ===${NC}"
echo "Default Web Interface: http://localhost:8492"
echo "Data Directory:        /var/lib/kadr"
echo "Config Directory:      /etc/kadr"
