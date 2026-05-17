#!/bin/bash
# Guest PID 1 Init Script - Hardened

# 1. Mount essential pseudo-filesystems
mount -t proc proc /proc
mount -t sysfs sys /sys

# 2. Mount the secondary metadata drive (vdb) read-only
mkdir -p /mnt/metadata
mount -o ro /dev/vdb /mnt/metadata

# 3. Load Secrets
if [ -f /mnt/metadata/.env ]; then
    # Export the variables so they are available for the agent
    export $(grep -v '^#' /mnt/metadata/.env | xargs)
fi

# 4. Cleanup metadata mount before starting agent
# This prevents the agent from re-reading secrets or exploring the drive
umount /mnt/metadata
rmdir /mnt/metadata

# 5. Guest Networking Setup
ip addr add 172.16.0.2/24 dev eth0
ip link set eth0 up
ip route add default via 172.16.0.1
echo "nameserver 8.8.8.8" > /etc/resolv.conf

echo "Init complete. Dropping privileges and starting Hermes..."

# 6. Drop Privileges and Execute
# -p preserves only the env variables we exported
exec runuser -u agent_user -p -- hermes
