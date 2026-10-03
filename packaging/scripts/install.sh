#!/usr/bin/env bash
set -euo pipefail

# Kadr Media Server Linux Standalone Installer
# Installs binary, web assets, configuration, and systemd service.

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}=== Installing Kadr Media Server ===${NC}"

if [ "$EUID" -ne 0 ]; then
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

# 1. Create system user and group
if ! id -u kadr >/dev/null 2>&1; then
    echo "Creating system user 'kadr'..."
    useradd --system --no-create-home --user-group --shell /usr/sbin/nologin kadr
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
fi

if [ ! -f /etc/kadr/kadr.toml ]; then
    if [ -f "${PARENT_DIR}/kadr.toml.example" ]; then
        cp "${PARENT_DIR}/kadr.toml.example" /etc/kadr/kadr.toml
    elif [ -f "${SCRIPT_DIR}/../systemd/kadr.toml.example" ]; then
        cp "${SCRIPT_DIR}/../systemd/kadr.toml.example" /etc/kadr/kadr.toml
    fi
fi

chmod 0640 /etc/kadr/* || true
chown -R root:kadr /etc/kadr || true

# 6. Install systemd service
SERVICE_SRC=""
if [ -f "${PARENT_DIR}/kadr.service" ]; then
    SERVICE_SRC="${PARENT_DIR}/kadr.service"
elif [ -f "${SCRIPT_DIR}/../systemd/kadr.service" ]; then
    SERVICE_SRC="${SCRIPT_DIR}/../systemd/kadr.service"
fi

if [ -n "${SERVICE_SRC}" ] && [ -d /etc/systemd/system ]; then
    echo "Installing systemd service..."
    cp "${SERVICE_SRC}" /etc/systemd/system/kadr.service
    systemctl daemon-reload
    echo -e "${GREEN}Service installed!${NC}"
    echo "To start Kadr now:       systemctl start kadr"
    echo "To enable on boot:       systemctl enable kadr"
    echo "To view live logs:       journalctl -u kadr -f"
fi

echo -e "${GREEN}=== Kadr installed successfully! ===${NC}"
echo "Default Web Interface: http://localhost:8492"
echo "Data Directory:        /var/lib/kadr"
echo "Config Directory:      /etc/kadr"
