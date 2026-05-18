# Spec 005: Debootstrap Browser Isolation (Steel Browser)

## 1. Overview
This specification details the transition from Docker-based rootfs generation to a direct, debootstrap-based approach for the Steel Browser. It also defines a new directory structure to keep the Hermes and Steel experiments isolated from each other.

## 2. Workspace Restructuring
To maintain clean separation between projects, the following directory structure will be implemented:

*   **`scripts/hermes/`**: Contains all existing scripts for the Hermes AI agent.
    *   `build-rootfs.sh`
    *   `create-metadata-drive.sh`
    *   `init-hermes.sh`
    *   `run-vm.sh`
    *   `setup-network.sh`
*   **`scripts/steel/`**: Contains new scripts for the Steel Browser experiments.
    *   `build-rootfs.sh` (Debootstrap implementation)
    *   `create-metadata-drive.sh` (Steel-specific metadata)
    *   `init-browser.sh` (Guest PID 1)
    *   `run-vm.sh` (Browser-specific launch with `virtio-rng`)

## 3. RootFS Generation (Debootstrap)
The Steel Browser rootfs will be built directly on the host to ensure a minimal, secure, and reproducible environment.

### 3.1 Base OS
- **Distribution:** Debian Bookworm (Stable)
- **Variant:** `minbase` or `default` (to be determined during implementation for size/dependency balance).

### 3.2 Guest Dependencies
The following packages must be installed within the chroot environment:
- **Runtime:** `nodejs`, `npm`, `ca-certificates`, `curl`, `git`.
- **Chromium/Playwright Shared Libs:** `libnss3`, `libatk1.0-0`, `libatk-bridge2.0-0`, `libcups2`, `libdrm2`, `libxcomposite1`, `libxdamage1`, `libxext6`, `libxfixes3`, `libxrandr2`, `libgbm1`, `libpango-1.0-0`, `libcairo2`, `libasound2`.
- **System:** `util-linux` (for `runuser`), `tini` (as a potential zombie reaper).

### 3.3 Steel Browser Build
1. Clone `https://github.com/steel-dev/steel-browser.git` into `/opt/steel-browser` inside the chroot.
2. Run `npm install` and `npm run build` (if applicable) within the chroot environment.
3. Configure the browser to run in headless mode and disable unnecessary features (e.g., GPU, sandbox if redundant with Firecracker).

## 4. Guest Initialization (`init-browser.sh`)
The `init-browser.sh` script will serve as PID 1 and perform the following:
1. Mount pseudo-filesystems (`/proc`, `/sys`).
2. Mount the metadata drive (`/dev/vdb`) read-only.
3. Load `STEEL_API_KEY` and any session configuration.
4. Unmount and remove the metadata drive mount point.
5. Configure guest networking (eth0) for the `browser` profile.
6. Drop privileges to an unprivileged `browser_user`.
7. Execute the Steel Browser API server.

## 5. VM Launch Requirements
The `scripts/steel/run-vm.sh` must include the following Firecracker configurations:
- **Memory:** Minimum 512MB (Recommended 1GB).
- **VCPU:** 2.
- **Drives:** Rootfs (ext4) and Metadata (ext4).
- **Network:** `tap0` with the `browser` iptables profile.
- **Entropy:** A `virtio-rng` device backed by the host's `/dev/urandom` to prevent TLS/SSL hangs.

## 6. Security & Isolation
- **Ephemeral State:** The rootfs image must be treated as a read-only template. Every launch should use a temporary copy or an overlay that is destroyed on exit.
- **Least Privilege:** The browser process must never run as root.
- **Egress Filtering:** Strictly enforce the `browser` profile (80/443 only, no private network access).
