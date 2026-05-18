#!/bin/bash
set -e

# Microvisor: Steel Browser - Debian Provider
# This script builds a minimal Debian rootfs for Steel Browser using debootstrap.

# Get the project root directory (two levels up from scripts/steel/build/)
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
ROOTFS_FILE="$PROJECT_ROOT/resources/browser-rootfs.ext4"
SIZE_MB=3072 # 3GB to accommodate Chromium and Node.js
DISTRO="bookworm"
TEMP_DIR=$(mktemp -d)

echo "[*] Ensuring resources directory exists..."
mkdir -p ../../../resources

# Ensure we cleanup on exit
trap 'sudo umount $TEMP_DIR/mnt 2>/dev/null || true; sudo rm -rf $TEMP_DIR' EXIT

echo "[1/5] Creating sparse ext4 image ($SIZE_MB MB)..."
truncate -s ${SIZE_MB}M $ROOTFS_FILE
mkfs.ext4 -F $ROOTFS_FILE

echo "[2/5] Running debootstrap (base OS)..."
mkdir -p $TEMP_DIR/mnt
sudo mount $ROOTFS_FILE $TEMP_DIR/mnt
sudo debootstrap --variant=minbase --include=ca-certificates,curl,git,util-linux,procps,tini,iproute2 $DISTRO $TEMP_DIR/mnt http://deb.debian.org/debian/

echo "[3/5] Installing dependencies and browser environment..."
# We use a heredoc to run commands inside the chroot
sudo chroot $TEMP_DIR/mnt /bin/bash <<EOF
set -e
export DEBIAN_FRONTEND=noninteractive

# Install Node.js (Current)
curl -fsSL https://deb.nodesource.com/setup_24.x | bash -
apt-get install -y nodejs

# Install Chromium and minimal UI dependencies
apt-get install -y --no-install-recommends \
    chromium \
    libnss3 \
    libatk1.0-0 \
    libatk-bridge2.0-0 \
    libcups2 \
    libdrm2 \
    libxcomposite1 \
    libxdamage1 \
    libxext6 \
    libxfixes3 \
    libxrandr2 \
    libgbm1 \
    libpango-1.0-0 \
    libcairo2 \
    libasound2 \
    fonts-liberation \
    libvulkan1 \
    xdg-utils

# Create browser user
useradd -m -s /bin/bash browser_user

# Setup browser directory
mkdir -p /opt/steel-browser
git clone --depth 1 https://github.com/steel-dev/steel-browser.git /opt/steel-browser

# Install dependencies and build
cd /opt/steel-browser
npm install
npm run build

# Setup permissions
chown -R browser_user:browser_user /opt/steel-browser
mkdir -p /home/browser_user/.cache
chown -R browser_user:browser_user /home/browser_user/.cache

# Cleanup apt
apt-get clean
rm -rf /var/lib/apt/lists/*
EOF

echo "[4/5] Injecting init script..."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
sudo cp "$SCRIPT_DIR/../init/init-browser.sh" $TEMP_DIR/mnt/usr/local/bin/init-browser.sh
sudo chmod +x $TEMP_DIR/mnt/usr/local/bin/init-browser.sh

echo "[5/5] Finalizing image..."
# Final sync and unmount handled by trap

echo "[+] Success! Debian RootFS built at $ROOTFS_FILE"
