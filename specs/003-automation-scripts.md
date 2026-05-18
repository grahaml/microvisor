# Spec: Automation Scripts

## Overview
This document defines the host-side shell scripts required to automate the manual phases of the Firecracker environment setup. These scripts serve as the prototypes for the eventual Rust-based orchestration plane.

## Script Registry

### 1. Hermes AI Agent (`scripts/hermes/`)
Automation for the Python-based agent prototype.

*   **`build-rootfs.sh`**: Docker-based extraction of the `hermes-agent` environment.
*   **`create-metadata-drive.sh`**: Injects Anthropic API keys and optional prompts.
*   **`init-hermes.sh`**: Guest PID 1 for secret loading and privilege dropping.
*   **`run-vm.sh`**: Standard microVM launch (1 vCPU, 512MB RAM).
*   **`setup-network.sh`**: Configures host-side TAP devices and iptables.

### 2. Steel Browser (`scripts/steel/`)
Automation for the high-density browser isolation prototype.

*   **`build-rootfs.sh`**: Direct debootstrap construction with Chromium/Node.js.
*   **`create-metadata-drive.sh`**: Injects Steel API keys.
*   **`init-browser.sh`**: Guest PID 1 for starting the Steel API server.
*   **`run-vm.sh`**: High-performance launch (2 vCPUs, 1024MB RAM, `virtio-rng`).

## Error Handling & Logging
- Each script must use `set -e` for fail-fast behavior.
- Use meaningful exit codes.
- Output progress to stdout and errors to stderr.
