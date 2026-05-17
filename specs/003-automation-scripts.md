# Spec: Automation Scripts

## Overview
This document defines the host-side shell scripts required to automate the manual phases of the Firecracker environment setup. These scripts serve as the prototypes for the eventual Rust-based orchestration plane.

## Script Registry

### 1. `scripts/build-rootfs.sh`
- **Purpose:** Automates Phase 2 (Custom RootFS via Docker).
- **Inputs:** `Dockerfile`, `scripts/init-hermes.sh`.
- **Outputs:** `resources/rootfs.ext4`.
- **Logic:**
    1. Build Docker image.
    2. Create empty image file.
    3. Format as ext4.
    4. Export Docker filesystem to image.
    5. Inject `init-hermes.sh` as `/sbin/init` or equivalent.

### 2. `scripts/create-metadata-drive.sh`
- **Purpose:** Automates Phase 3 (Secret Injection).
- **Inputs:** API Key (via argument).
- **Outputs:** `resources/metadata.ext4`.
- **Logic:**
    1. Create 2MB blank image.
    2. Format as ext4.
    3. Write `.env` file containing secrets.

### 3. `scripts/setup-network.sh`
- **Purpose:** Automates Phase 4 (Host Networking).
- **Inputs:** None (uses host environment discovery).
- **Outputs:** Active `tap0` interface, iptables rules.
- **Logic:**
    1. Create TAP device.
    2. Assign IP `172.16.0.1`.
    3. Enable sysctl forwarding.
    4. Apply NAT/Masquerade rules.

### 4. `scripts/run-vm.sh`
- **Purpose:** Automates Phase 5 (VM Lifecycle).
- **Inputs:** `resources/vmlinux.bin`, `resources/rootfs.ext4`, `resources/metadata.ext4`.
- **Outputs:** Running Firecracker process.
- **Logic:**
    1. Cleanup stale sockets.
    2. Launch Firecracker binary.
    3. API: Set boot source.
    4. API: Attach drives (vda, vdb).
    5. API: Attach network interface.
    6. API: Start instance.

## Error Handling & Logging
- Each script must use `set -e` for fail-fast behavior.
- Use meaningful exit codes.
- Output progress to stdout and errors to stderr.
