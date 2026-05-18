#!/bin/bash
set -e

# Microvisor: Steel Browser - Alpine Provider
# Target: Ultra-low latency, sub-second cold boot.

ROOTFS_FILE="../../../resources/browser-alpine.ext4"
SIZE_MB=512 # Alpine is tiny
TEMP_DIR=$(mktemp -d)
REPO="https://dl-cdn.alpinelinux.org/alpine/v3.19/main"

echo "[*] Ensuring resources directory exists..."
mkdir -p ../../../resources

# Ensure we cleanup on exit
trap 'sudo umount $TEMP_DIR/mnt 2>/dev/null || true; sudo rm -rf $TEMP_DIR' EXIT

echo "[1/4] Creating sparse ext4 image ($SIZE_MB MB)..."
truncate -s ${SIZE_MB}M $ROOTFS_FILE
mkfs.ext4 -F $ROOTFS_FILE

echo "[2/4] Installing Alpine base (using apk)..."
mkdir -p $TEMP_DIR/mnt
sudo mount $ROOTFS_FILE $TEMP_DIR/mnt

# We use the static apk from the host if available, or download a mini-rootfs
# For simplicity in this script, we'll assume the host has 'apk' (e.g. running on Alpine or via package)
# If not, we download the mini-rootfs and extract it.
if ! command -v apk &> /dev/null; then
    echo "[!] 'apk' command not found on host. Downloading mini-rootfs..."
    curl -fsSL https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86_64/alpine-minirootfs-3.19.1-x86_64.tar.gz | sudo tar -xzC $TEMP_DIR/mnt
else
    sudo apk add --root $TEMP_DIR/mnt --initdb --repository $REPO alpine-base
fi

echo "[3/4] Configuring guest environment..."
sudo chroot $TEMP_DIR/mnt /bin/sh <<EOF
set -e
# Install essential packages
apk add --no-cache \
    bash \
    util-linux \
    iproute2 \
    ca-certificates \
    tini \
    nodejs \
    npm \
    chromium \
    nss \
    freetype \
    harfbuzz \
    ttf-freefont

# Create browser user
adduser -D -s /bin/bash browser_user

# Build Steel Browser from source
cd /opt
# Note: In a real scenario, we'd copy the source or download a release
# git clone --depth 1 https://github.com/steel-dev/steel-browser.git
mkdir -p steel-browser
cd steel-browser
# For the prototype, we assume the browser code is injected or pre-built
# npm install --production

# Setup permissions
chown -R browser_user:browser_user /opt/steel-browser
EOF

echo "[4/4] Injecting init script..."
sudo cp ../init/init-browser.sh $TEMP_DIR/mnt/usr/local/bin/init-browser.sh
sudo chmod +x $TEMP_DIR/mnt/usr/local/bin/init-browser.sh
# Fix bash path for Alpine (init-browser.sh uses #!/bin/bash)
sudo sed -i 's/\/bin\/bash/\/bin\/sh/g' $TEMP_DIR/mnt/usr/local/bin/init-browser.sh

echo "[+] Success! Alpine RootFS built at $ROOTFS_FILE"
