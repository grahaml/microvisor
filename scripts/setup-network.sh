#!/usr/bin/env bash
set -e

echo "[*] Setting up network for Firecracker microVMs..."

TAP_DEV="tap0"
TAP_IP="172.16.0.1/24"

echo "[*] Detecting primary internet interface..."
PRIMARY_IFACE=$(ip route | grep default | awk '{print $5}' | head -n1)

if [ -z "$PRIMARY_IFACE" ]; then
    echo "[-] Error: Could not detect primary network interface." >&2
    exit 1
fi
echo "[+] Primary interface detected: $PRIMARY_IFACE"

echo "[*] Creating TAP device '$TAP_DEV'..."
if ! ip link show "$TAP_DEV" > /dev/null 2>&1; then
    sudo ip tuntap add dev "$TAP_DEV" mode tap
else
    echo "[+] TAP device '$TAP_DEV' already exists."
fi

echo "[*] Assigning IP $TAP_IP to '$TAP_DEV'..."
sudo ip addr add "$TAP_IP" dev "$TAP_DEV" 2>/dev/null || echo "[+] IP already assigned or device in use."
sudo ip link set dev "$TAP_DEV" up

echo "[*] Enabling IPv4 forwarding..."
sudo sysctl -w net.ipv4.ip_forward=1

echo "[*] Configuring iptables for NAT and forwarding..."
# Add MASQUERADE rule if it doesn't already exist
if ! sudo iptables -t nat -C POSTROUTING -o "$PRIMARY_IFACE" -j MASQUERADE 2>/dev/null; then
    sudo iptables -t nat -A POSTROUTING -o "$PRIMARY_IFACE" -j MASQUERADE
fi

# Add FORWARD rules if they don't already exist
if ! sudo iptables -C FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT 2>/dev/null; then
    sudo iptables -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
fi

if ! sudo iptables -C FORWARD -i "$TAP_DEV" -o "$PRIMARY_IFACE" -j ACCEPT 2>/dev/null; then
    sudo iptables -A FORWARD -i "$TAP_DEV" -o "$PRIMARY_IFACE" -j ACCEPT
fi

echo "[+] Network setup successfully completed!"
