# Spec: Networking Infrastructure

## Overview
Firecracker microVMs are isolated by default. This subsystem provides the virtual "plumbing" to connect microVMs to each other and the external internet securely.

## Topology

### 1. Host TAP Device (`tap0`)
A virtual network interface on the host machine that acts as the "gateway" for the microVM.
*   **IP Address:** `172.16.0.1` (Gateway).
*   **Subnet:** `172.16.0.0/24`.

### 2. Guest Ethernet Interface (`eth0`)
The network interface inside the microVM.
*   **IP Address:** `172.16.0.2` (for the first instance).
*   **MAC Address:** Must be unique per VM instance.

## Host Configuration (NAT/Routing)

### 1. IP Forwarding
The host Linux kernel must have IPv4 forwarding enabled (`net.ipv4.ip_forward=1`) to pass packets between the `tap0` interface and the host's physical network interface.

### 2. IP Masquerading (NAT)
To allow microVMs to reach the internet (e.g., Anthropic API), the host must perform Network Address Translation (NAT) using `iptables` or `nftables`. This "hides" the private `172.16.0.x` addresses behind the host's public/primary IP.

### 3. DNS
The guest VM must be configured with a DNS server (e.g., `8.8.8.8` or the host's DNS resolver) to resolve hostnames.

## Requirements
- [ ] Create a `tap0` device on the host.
- [ ] Assign the gateway IP to `tap0`.
- [ ] Enable IP forwarding on the host.
- [ ] Configure `iptables` for MASQUERADE and FORWARD rules.
- [ ] Guest `init.sh` must configure its internal IP and default route to the host gateway.
