#!bin/bash
mount -t proc proc /proc
mount -t sysfs sys /sys
mkdir -p /mnt/metadata
mount /dev/vdb /mnt/metadata
export $(grep -v '^#' /mnt/metadata/.env | xargs)
ip addr add 172.16.0.2/24 dev eth0
ip link set eth0 up
ip route add default via 172.16.0.1
echo "nameserver 8.8.8.8" > /etc/resolv.conf
echo "Starting Hermes..."
exec /usr/bin/hermes
