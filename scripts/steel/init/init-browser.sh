#!/bin/bash
# Guest PID 1 Init Script (Steel Browser)
set -x

# 1. Setup PATH
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# 2. Mount essential pseudo-filesystems
mount -t proc proc /proc
mount -t sysfs sys /sys

# 3. Mount the secondary metadata drive (vdb) read-only
mkdir -p /mnt/metadata
mount -o ro /dev/vdb /mnt/metadata

# 4. Load Secrets (Steel API Key)
if [ -f /mnt/metadata/.env ]; then
    export $(grep -v '^#' /mnt/metadata/.env | xargs)
fi

# 5. Cleanup metadata mount
umount /mnt/metadata
rmdir /mnt/metadata

# 6. Guest Networking Setup (Browser Profile)
# Matches the setup in scripts/hermes/setup-network.sh for tap0
ip addr add 172.16.0.2/24 dev eth0
ip link set eth0 up
ip route add default via 172.16.0.1
echo "nameserver 8.8.8.8" > /etc/resolv.conf

echo "Init complete. Starting Steel Browser..."
node -v
npm -v

# 7. Drop Privileges and Execute Steel Browser
# We use 'tini' to handle signal forwarding and zombie reaping
if [ -z "$STEEL_API_KEY" ]; then
    echo "[!] Starting Steel Browser in LOCAL mode (no API key required)."
    exec runuser -l browser_user -c "cd /opt/steel-browser; exec tini -s -- npm start -w api"
else
    echo "[+] Starting Steel Browser with provided API key."
    exec runuser -l browser_user -c "export STEEL_API_KEY=\"$STEEL_API_KEY\"; cd /opt/steel-browser; exec tini -s -- npm start -w api"
fi
