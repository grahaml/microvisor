#!/usr/bin/env bash
# Steel Browser - Network Setup
set -e

# This script prepares the host networking for the Steel Browser microVM.
# It uses a dedicated TAP device and applies egress filtering for browser isolation.

echo "[*] Setting up network for Steel Browser..."

TAP_DEV="tap0"
TAP_IP="172.16.0.1"
TAP_CIDR="172.16.0.1/24"

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

echo "[*] Assigning IP $TAP_CIDR to '$TAP_DEV'..."
sudo ip addr add "$TAP_CIDR" dev "$TAP_DEV" 2>/dev/null || echo "[+] IP already assigned."
sudo ip link set dev "$TAP_DEV" up

echo "[*] Enabling IPv4 forwarding..."
sudo sysctl -w net.ipv4.ip_forward=1 > /dev/null

echo "[*] Configuring iptables for Browser Isolation..."
# Set default FORWARD policy to DROP for security
sudo iptables -P FORWARD DROP

# Flush existing rules for this TAP to avoid accumulation
sudo iptables -F FORWARD 2>/dev/null || true

# 1. Allow NAT (Masquerade) for outbound traffic
if ! sudo iptables -t nat -C POSTROUTING -o "$PRIMARY_IFACE" -j MASQUERADE 2>/dev/null; then
    sudo iptables -t nat -A POSTROUTING -o "$PRIMARY_IFACE" -j MASQUERADE
fi

# 2. Allow DNS (UDP 53) to the gateway
sudo iptables -A FORWARD -i "$TAP_DEV" -d "$TAP_IP" -p udp --dport 53 -j ACCEPT

# 3. Block access to the host machine (Anti-SSRF)
sudo iptables -A FORWARD -i "$TAP_DEV" -d "$TAP_IP" -j REJECT

# 4. Block access to other private/metadata ranges (Anti-SSRF)
for DROP_CIDR in "10.0.0.0/8" "172.16.0.0/12" "192.168.0.0/16" "169.254.169.254/32"; do
    sudo iptables -A FORWARD -i "$TAP_DEV" -d "$DROP_CIDR" -j REJECT
done

# 5. Allow arbitrary outbound HTTP/HTTPS for the browser
sudo iptables -A FORWARD -i "$TAP_DEV" -o "$PRIMARY_IFACE" -p tcp -m multiport --dports 80,443 -j ACCEPT

# 6. Allow return traffic for established connections
sudo iptables -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT

echo "[+] Steel Browser network setup complete."
