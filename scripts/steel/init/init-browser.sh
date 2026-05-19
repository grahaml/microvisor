#!/bin/bash
# Guest PID 1 Init Script (Steel Browser)
set -e

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

# 6. Guest Networking Setup
echo "[*] Diagnostic: Current network interfaces:"
ip link
ls /sys/class/net

echo "[*] Configuring loopback..."
ip link set lo up
ip addr add 127.0.0.1/8 dev lo || true
echo "127.0.0.1 localhost" > /etc/hosts

echo "[*] Configuring eth0..."
# Attempt to find the first non-loopback interface if eth0 is missing
NET_IFACE=$(ls /sys/class/net | grep -v lo | head -n 1)
if [ -z "$NET_IFACE" ]; then
    echo "[-] ERROR: No network interface found!"
else
    echo "[+] Using interface: $NET_IFACE"
    ip addr add 172.16.0.2/24 dev $NET_IFACE || echo "IP already assigned"
    ip link set $NET_IFACE up
    ip route add default via 172.16.0.1 || echo "Route already exists"
fi
echo "nameserver 8.8.8.8" > /etc/resolv.conf

echo "[+] Init complete. Starting Steel Browser API..."

# 7. Drop Privileges and Execute Steel Browser API
export STEEL_DIR="/opt/steel-browser/api"
NODE_BIN=$(which node)

if [ -z "$STEEL_API_KEY" ]; then
    echo "[!] Starting Steel Browser in LOCAL mode."
    exec runuser -l browser_user -c "export DEBUG_CHROME_PROCESS=true; export DISABLE_CHROME_SANDBOX=true; export SKIP_FINGERPRINT_INJECTION=true; cd $STEEL_DIR; exec tini -s -- $NODE_BIN build/index.js"
else
    echo "[+] Starting Steel Browser with API key."
    exec runuser -l browser_user -c "export DEBUG_CHROME_PROCESS=true; export DISABLE_CHROME_SANDBOX=true; export SKIP_FINGERPRINT_INJECTION=true; export STEEL_API_KEY=\"$STEEL_API_KEY\"; cd $STEEL_DIR; exec tini -s -- $NODE_BIN build/index.js"
fi
