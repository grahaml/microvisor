#!/bin/bash
set -e

# build-rootfs.sh (Steel Browser - Debootstrap version)
# This script builds a minimal Debian rootfs for Steel Browser without using Docker.

ROOTFS_FILE="../../../resources/browser-rootfs.ext4"
SIZE_MB=3072 # 3GB to accommodate Chromium and Node.js
DISTRO="bookworm"
TEMP_DIR=$(mktemp -d)

echo "[*] Ensuring resources directory exists..."
mkdir -p ../../../resources

# Ensure we cleanup on exit
trap 'sudo umount $TEMP_DIR/mnt 2>/dev/null || true; sudo rm -rf $TEMP_DIR' EXIT

echo "[1/5] Creating empty ext4 image ($SIZE_MB MB)..."
dd if=/dev/zero of=$ROOTFS_FILE bs=1M count=$SIZE_MB status=progress
mkfs.ext4 -F $ROOTFS_FILE

echo "[2/5] Running debootstrap (base OS)..."
mkdir -p $TEMP_DIR/mnt
sudo mount $ROOTFS_FILE $TEMP_DIR/mnt
sudo debootstrap --variant=minbase $DISTRO $TEMP_DIR/mnt http://deb.debian.org/debian/

echo "[3/5] Installing dependencies and browser environment..."
# We use a heredoc to run commands inside the chroot
sudo chroot $TEMP_DIR/mnt /bin/bash <<EOF
set -e
export DEBIAN_FRONTEND=noninteractive

# Update and install essential tools
apt-get update
apt-get install -y curl git ca-certificates util-linux procps tini

# Install Node.js (LTS)
curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
apt-get install -y nodejs

# Install Chromium and dependencies
apt-get install -y \
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
    libappindicator3-1 \
    libvulkan1 \
    xdg-utils

# Create browser user
useradd -m -s /bin/bash browser_user

# Build Steel Browser from source
cd /opt
git clone https://github.com/steel-dev/steel-browser.git
cd steel-browser
npm install
# npm run build  # Uncomment if the version needs a build step

# Setup permissions
chown -R browser_user:browser_user /opt/steel-browser
mkdir -p /home/browser_user/.cache
chown -R browser_user:browser_user /home/browser_user/.cache

# Cleanup apt
apt-get clean
rm -rf /var/lib/apt/lists/*
EOF

echo "[4/5] Injecting init script..."
sudo cp ../init/init-browser.sh $TEMP_DIR/mnt/usr/local/bin/init-browser.sh
sudo chmod +x $TEMP_DIR/mnt/usr/local/bin/init-browser.sh

echo "[5/5] Finalizing image..."
# Final sync and unmount handled by trap

echo "[+] Success! Browser RootFS built at $ROOTFS_FILE"
