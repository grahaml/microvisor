# Spec 005: Multi-Distro Browser Isolation & Benchmarking

## 1. Overview
This specification defines a modular architecture for building and benchmarking Steel Browser rootfs images across multiple Linux distributions. The goal is to determine the optimal balance between boot latency, compatibility, and resource density for the "Microvisor" orchestrator.

## 2. Workspace Restructuring
To support multiple distributions, the `scripts/steel/` directory will follow a modular provider pattern:

*   **`scripts/steel/build/`**: Distribution-specific build scripts.
    *   `debian.sh`: Debootstrap-based glibc baseline.
    *   `alpine.sh`: APK-based musl performance target.
    *   `wolfi.sh`: Apko-based secure glibc target.
*   **`scripts/steel/init/`**: Guest initialization templates.
    *   `init-browser.sh`: Universal PID 1 script (adapted per distro).
*   **`scripts/steel/bench/`**: Benchmarking suite.
    *   `latency-test.sh`: Measures time-to-ready for various images.
*   **`scripts/steel/run-vm.sh`**: Unified launch script supporting a `--distro` flag.

## 3. Multi-Distro RootFS Generation
Each provider must generate a `browser-rootfs-[distro].ext4` image satisfying the following requirements:

### 3.1 Distribution Targets
1.  **Debian (Baseline)**: High compatibility, glibc-based, larger footprint.
2.  **Alpine (Performance)**: Minimal footprint, musl-based, sub-second boot target.
3.  **Wolfi (Security/Glibc)**: Modern, stripped-down, glibc-based.

### 3.2 Unified Dependencies
Regardless of the distro, the image must contain:
- **Runtime**: Node.js (LTS), NPM.
- **Engine**: Chromium/Playwright with all required shared libraries.
- **Agent**: Steel Browser built from source in `/opt/steel-browser`.
- **Privilege Management**: `util-linux` (for `runuser`) or equivalent.

## 4. Guest Initialization
The initialization process must be standardized across distributions to ensure benchmarking consistency:
1. Mount pseudo-filesystems.
2. Securely ingest `STEEL_API_KEY` from `/dev/vdb`.
3. Configure `eth0` point-to-point networking.
4. **Fast-Drop**: Immediately drop privileges to `browser_user`.
5. **Direct Exec**: Execute the Steel API server directly (avoiding heavy shell wrappers where possible).

## 5. VM Launch & Hardware Fencing
The launch script must enforce the following "Microvisor" resource constraints:
- **Entropy**: `virtio-rng` device is MANDATORY.
- **CPU**: 2 vCPUs pinned to host physical cores (if orchestrator supports it).
- **Memory**: 1024MB RAM, pinned to local NUMA node.

## 6. Benchmarking & Latency Analysis
The benchmarking suite (`scripts/steel/bench/latency-test.sh`) will measure:
1.  **Image Size**: Impact on host CoW/copy latency.
2.  **Kernel Boot Time**: `InstanceStart` to `init` start.
3.  **User-Space Ready**: `init` start to `Steel API Port 3000` listening.
4.  **TLS Handshake Latency**: First HTTPS request time (verifying entropy efficiency).

## 7. Security & Isolation
- **Statelessness**: Every benchmark run must use a fresh, ephemeral copy of the rootfs.
- **Egress**: Strictly enforce the `browser` network profile during tests.
