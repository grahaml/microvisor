#!/usr/bin/env bash
set -e

PROFILE=${1:-"strict"} # Default to strict profile

if [[ "$PROFILE" != "strict" && "$PROFILE" != "browser" ]]; then
    echo "[-] Error: Profile must be 'strict' or 'browser'" >&2
    exit 1
fi

echo "[*] Setting up network for Firecracker microVMs (Profile: $PROFILE)..."

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

echo "[*] Flushing old rules for $TAP_DEV..."
# Clean slate for our specific TAP device
sudo iptables -D FORWARD -i "$TAP_DEV" -j ACCEPT 2>/dev/null || true
sudo iptables -D FORWARD -i "$TAP_DEV" -o "$PRIMARY_IFACE" -j ACCEPT 2>/dev/null || true

echo "[*] Configuring Base NAT (Masquerade)..."
if ! sudo iptables -t nat -C POSTROUTING -o "$PRIMARY_IFACE" -j MASQUERADE 2>/dev/null; then
    sudo iptables -t nat -A POSTROUTING -o "$PRIMARY_IFACE" -j MASQUERADE
fi

echo "[*] Applying SSRF Protection..."
# Explicitly drop traffic trying to reach private ranges or cloud metadata from the VM
for DROP_CIDR in "10.0.0.0/8" "172.16.0.0/12" "192.168.0.0/16" "169.254.169.254/32"; do
    # Allow traffic specifically to the Gateway IP so DNS/routing works
    if ! sudo iptables -C FORWARD -i "$TAP_DEV" -d "$TAP_IP" -j ACCEPT 2>/dev/null; then
        sudo iptables -I FORWARD 1 -i "$TAP_DEV" -d "$TAP_IP" -j ACCEPT
    fi
    # Block everything else in the private ranges
    if ! sudo iptables -C FORWARD -i "$TAP_DEV" -d "$DROP_CIDR" -j DROP 2>/dev/null; then
        sudo iptables -A FORWARD -i "$TAP_DEV" -d "$DROP_CIDR" -j DROP
    fi
done

echo "[*] Applying Egress Profile: $PROFILE..."

if [ "$PROFILE" == "strict" ]; then
    # Strict Profile: Allow only established connections and outbound HTTPS to specific API IPs
    # Note: Resolving api.anthropic.com here is a slight hack for the shell script POC. 
    # In Rust, we would use an explicit IP allowlist or an egress proxy.
    API_IP=$(dig +short api.anthropic.com | head -n1)
    
    if [ -n "$API_IP" ]; then
        if ! sudo iptables -C FORWARD -i "$TAP_DEV" -d "$API_IP" -p tcp --dport 443 -j ACCEPT 2>/dev/null; then
            sudo iptables -A FORWARD -i "$TAP_DEV" -d "$API_IP" -p tcp --dport 443 -j ACCEPT
        fi
        echo "[+] Strict rules applied (Allowed IP: $API_IP:443)"
    else
        echo "[-] Warning: Could not resolve Anthropic API. Egress is fully blocked."
    fi

    # Explicit default deny for this TAP interface
    sudo iptables -A FORWARD -i "$TAP_DEV" -j REJECT

elif [ "$PROFILE" == "browser" ]; then
    # Browser Profile: Allow arbitrary 80/443 traffic (SSRF rules above protect us)
    if ! sudo iptables -C FORWARD -i "$TAP_DEV" -o "$PRIMARY_IFACE" -p tcp -m multiport --dports 80,443 -j ACCEPT 2>/dev/null; then
        sudo iptables -A FORWARD -i "$TAP_DEV" -o "$PRIMARY_IFACE" -p tcp -m multiport --dports 80,443 -j ACCEPT
    fi
    echo "[+] Browser rules applied (Allowed ports 80/443)"
    
    # Explicit default deny for anything else (e.g. SMTP, SSH)
    sudo iptables -A FORWARD -i "$TAP_DEV" -j REJECT
fi

# Always allow return traffic
if ! sudo iptables -C FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT 2>/dev/null; then
    sudo iptables -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
fi

echo "[+] Network setup successfully completed!"
