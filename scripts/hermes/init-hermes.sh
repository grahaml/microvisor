#!/bin/bash
# Guest PID 1 Init Script - Hardened
set -x

# 1. Setup PATH
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# 2. Mount essential pseudo-filesystems
mount -t proc proc /proc
mount -t sysfs sys /sys

# 3. Mount the secondary metadata drive (vdb) read-only
mkdir -p /mnt/metadata
mount -o ro /dev/vdb /mnt/metadata

# 4. Load Secrets and Prompt
if [ -f /mnt/metadata/.env ]; then
    # Export the variables so they are available for the agent
    export $(grep -v '^#' /mnt/metadata/.env | xargs)
fi

AGENT_PROMPT=""
if [ -f /mnt/metadata/prompt.txt ]; then
    AGENT_PROMPT=$(cat /mnt/metadata/prompt.txt)
fi

# 5. Cleanup metadata mount before starting agent
# This prevents the agent from re-reading secrets or exploring the drive
umount /mnt/metadata
rmdir /mnt/metadata

# 6. Guest Networking Setup
ip addr add 172.16.0.2/24 dev eth0
ip link set eth0 up
ip route add default via 172.16.0.1
echo "nameserver 8.8.8.8" > /etc/resolv.conf

echo "Init complete. Dropping privileges and starting Hermes..."

# 7. Drop Privileges and Execute
# -l (login) ensures HOME and other environment variables are set correctly
# We explicitly pass ANTHROPIC_API_KEY into the subshell
if [ -n "$AGENT_PROMPT" ]; then
    exec runuser -l agent_user -c "export ANTHROPIC_API_KEY=\"$ANTHROPIC_API_KEY\"; hermes -z \"$AGENT_PROMPT\""
else
    exec runuser -l agent_user -c "export ANTHROPIC_API_KEY=\"$ANTHROPIC_API_KEY\"; hermes"
fi
