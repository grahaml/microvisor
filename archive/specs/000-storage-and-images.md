# Spec: Storage and Image Management

## Overview
This subsystem is responsible for creating and managing the disk images used by Firecracker microVMs. It ensures that agents have a clean, reproducible operating system environment and a secure way to receive secrets/state.

## Components

### 1. Base RootFS (`rootfs.ext4`)
The primary system drive for the microVM. 
*   **Source:** A Docker container (e.g., Python 3.12-slim).
*   **Build Process:** 
    1.  Define environment in a `Dockerfile`.
    2.  Use `docker export` to capture the filesystem.
    3.  Create a raw `.ext4` file of a fixed size (e.g., 2GB).
    4.  Format the file and copy the exported files into it.
*   **Responsibility:** Provides the kernel with the necessary binaries, libraries, and the custom `init` process (`/usr/local/bin/init.sh`).

### 2. Secondary Metadata Drive (`metadata.ext4`)
A tiny, volatile drive used to inject ephemeral data (like API keys) into a specific VM instance.
*   **Source:** Dynamically generated per-instance.
*   **Size:** Minimal (e.g., 2MB).
*   **Contents:** An `.env` file containing secrets like `ANTHROPIC_API_KEY`.
*   **Mount Point:** Attached as `/dev/vdb` and mounted by the guest `init.sh` to `/mnt/metadata`.

### 3. Guest Init Process (`init.sh`)
The custom entrypoint for the microVM (PID 1).
*   **Logic:**
    1.  Mount essential pseudo-filesystems (`/proc`, `/sys`).
    2.  Identify and mount the secondary metadata drive (`/dev/vdb`).
    3.  Source the environment variables from the metadata drive.
    4.  Configure the guest network interface (`eth0`) based on the project's IP scheme.
    5.  Execute the agent process (e.g., `hermes`).

## Success Criteria
- [ ] A reproducible script can build a `rootfs.ext4` from a Dockerfile.
- [ ] A script can generate a `metadata.ext4` containing an arbitrary API key.
- [ ] The resulting images are compatible with the Firecracker block device model.
