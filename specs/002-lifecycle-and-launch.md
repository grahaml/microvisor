# Spec: VM Lifecycle and Launch

## Overview
This subsystem manages the lifecycle of a Firecracker microVM instance, from initial configuration to startup and eventual shutdown. It interacts with the Firecracker REST API over a Unix Domain Socket.

## Lifecycle Phases

### 1. Preparation
Before starting Firecracker, the host must ensure:
- The `bin/firecracker` binary is available.
- The `resources/vmlinux.bin` kernel is present.
- The `resources/rootfs.ext4` system image is built.
- The `resources/metadata.ext4` instance image is generated.
- The host networking (`tap0`) is configured.

### 2. Initialization
1.  Clean up any existing Unix Domain Socket (`/tmp/firecracker.socket`).
2.  Start the `firecracker` process in the background, listening on the socket.

### 3. Configuration (REST API)
The orchestrator must send several `PUT` requests to the Firecracker API:
*   **Boot Source:** Set the path to the kernel and the boot arguments (e.g., `console=ttyS0 reboot=k panic=1 pci=off`).
*   **Root Drive:** Attach `rootfs.ext4` as a block device.
*   **Metadata Drive:** Attach `metadata.ext4` as a second block device.
*   **Network Interface:** Attach the host's `tap0` device to the guest's `eth0`.

### 4. Startup
Send an `InstanceStart` action to the `/actions` endpoint to "power on" the microVM.

### 5. Monitoring & Termination
- Monitor the Firecracker process.
- Shutdown is typically initiated from within the guest (e.g., `reboot` or `poweroff`) or by killing the host Firecracker process.

## Requirements
- [ ] A script (`run_vm.sh`) that automates the API calls.
- [ ] Proper error handling for failed API responses.
- [ ] Cleanup logic to ensure the socket is removed on exit.
