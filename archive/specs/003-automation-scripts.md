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

*   **`build/`**: Contains distribution-specific rootfs providers.
    *   `debian.sh`: Debootstrap-based construction.
    *   `alpine.sh`: APK-based construction (musl).
    *   `wolfi.sh`: Apko-based construction.
*   **`create-metadata-drive.sh`**: Injects Steel API keys.
*   **`init-browser.sh`**: Universal guest PID 1 template.
*   **`bench/latency-test.sh`**: Measures boot-to-ready latency across distros.
*   **`run-vm.sh`**: Unified launch script with `--distro` support and `virtio-rng`.

## Error Handling & Logging
- Each script must use `set -e` for fail-fast behavior.
- Use meaningful exit codes.
- Output progress to stdout and errors to stderr.
