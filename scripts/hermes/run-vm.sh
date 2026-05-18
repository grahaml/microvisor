#!/bin/bash
set -e

# Hardened VM Launch Script
KERNEL="resources/vmlinux.bin"
ROOTFS="resources/rootfs.ext4"
METADATA="resources/metadata.ext4"
TAP_DEV="tap0"
API_SOCKET="/tmp/firecracker.socket"

# 1. Cleanup and Traps
rm -f $API_SOCKET
# Ensure we cleanup the socket and kill Firecracker if the script is interrupted (Ctrl+C)
trap 'rm -f $API_SOCKET; [ -n "$FC_PID" ] && kill $FC_PID 2>/dev/null' EXIT

# 2. Start Firecracker
./bin/firecracker --api-sock $API_SOCKET &
FC_PID=$!
sleep 0.5

# Restrict permissions on the socket so only YOU can talk to it
chmod 600 $API_SOCKET

echo "[*] Configuring VM..."

# 3. Boot Source
curl --unix-socket $API_SOCKET -X PUT 'http://localhost/boot-source' \
  -H 'Content-Type: application/json' \
  -d "{
        \"kernel_image_path\": \"$KERNEL\",
        \"boot_args\": \"console=ttyS0 reboot=k panic=1 pci=off rw init=/usr/local/bin/init-hermes.sh\"
    }"

# 4. Root Drive
curl --unix-socket $API_SOCKET -X PUT 'http://localhost/drives/rootfs' \
  -H 'Content-Type: application/json' \
  -d "{
        \"drive_id\": \"rootfs\",
        \"path_on_host\": \"$ROOTFS\",
        \"is_root_device\": true,
        \"is_read_only\": false
    }"

# 5. Metadata Drive
curl --unix-socket $API_SOCKET -X PUT 'http://localhost/drives/metadata' \
  -H 'Content-Type: application/json' \
  -d "{
        \"drive_id\": \"metadata\",
        \"path_on_host\": \"$METADATA\",
        \"is_root_device\": false,
        \"is_read_only\": true
    }"

# 6. Network Interface
curl --unix-socket $API_SOCKET -X PUT 'http://localhost/network-interfaces/eth0' \
  -H 'Content-Type: application/json' \
  -d "{
        \"iface_id\": \"eth0\",
        \"guest_mac\": \"AA:FC:00:00:00:01\",
        \"host_dev_name\": \"$TAP_DEV\"
    }"

echo "[+] Powering on..."
curl --unix-socket $API_SOCKET -X PUT 'http://localhost/actions' \
  -H 'Content-Type: application/json' \
  -d '{"action_type": "InstanceStart"}'

# Keep script running to see output and maintain the trap
wait $FC_PID
