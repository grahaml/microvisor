#!/bin/bash
set -e

# run-vm.sh (Steel Browser)
# Launches a Firecracker microVM with 1GB RAM, 2 vCPUs, and virtio-rng.

cd "$(dirname "$0")/../.."

KERNEL="resources/vmlinux-6.1.bin"
ROOTFS="resources/browser-rootfs.ext4"
METADATA="resources/metadata.ext4"
TAP_DEV="tap0"
API_SOCKET="/tmp/firecracker-browser.socket"

# 1. Cleanup and Traps
rm -f $API_SOCKET
trap 'rm -f $API_SOCKET; [ -n "$FC_PID" ] && kill $FC_PID 2>/dev/null' EXIT

# 2. Start Firecracker
./bin/firecracker --api-sock $API_SOCKET &
FC_PID=$!
sleep 0.5

chmod 600 $API_SOCKET

echo "[*] Configuring Steel Browser VM..."

# 3. Boot Source
# We use 'random.trust_cpu=on' for instant entropy on 6.x kernels
# 3. Boot Source
curl --fail --silent --unix-socket $API_SOCKET -X PUT 'http://localhost/boot-source' \
  -H 'Content-Type: application/json' \
  -d "{
        \"kernel_image_path\": \"$KERNEL\",
        \"boot_args\": \"console=ttyS0 reboot=k panic=1 pci=off rw root=/dev/vda init=/usr/local/bin/init-browser.sh\"
    }"

# 4. Machine Config (Higher specs for browser)
curl --fail --silent --unix-socket $API_SOCKET -X PUT 'http://localhost/machine-config' \
  -H 'Content-Type: application/json' \
  -d '{
        "vcpu_count": 2,
        "mem_size_mib": 1024,
        "smt": false
    }'

# 5. Root Drive
curl --fail --silent --unix-socket $API_SOCKET -X PUT 'http://localhost/drives/rootfs' \
  -H 'Content-Type: application/json' \
  -d "{
        \"drive_id\": \"rootfs\",
        \"path_on_host\": \"$ROOTFS\",
        \"is_root_device\": true,
        \"is_read_only\": false
    }"

# 6. Metadata Drive
curl --fail --silent --unix-socket $API_SOCKET -X PUT 'http://localhost/drives/metadata' \
  -H 'Content-Type: application/json' \
  -d "{
        \"drive_id\": \"metadata\",
        \"path_on_host\": \"$METADATA\",
        \"is_root_device\": false,
        \"is_read_only\": true
    }"

# 7. Network Interface
curl --fail --silent --unix-socket $API_SOCKET -X PUT 'http://localhost/network-interfaces/eth0' \
  -H 'Content-Type: application/json' \
  -d "{
        \"iface_id\": \"eth0\",
        \"guest_mac\": \"AA:FC:00:00:00:02\",
        \"host_dev_name\": \"$TAP_DEV\"
    }"

# 8. Entropy Device (Crucial for Browser/TLS)
curl --fail --silent --unix-socket $API_SOCKET -X PUT 'http://localhost/entropy' \
  -H 'Content-Type: application/json' \
  -d '{}'

echo "[+] Powering on..."
curl --fail --silent --unix-socket $API_SOCKET -X PUT 'http://localhost/actions' \
  -H 'Content-Type: application/json' \
  -d '{"action_type": "InstanceStart"}'

# Keep script running to see output
wait $FC_PID
