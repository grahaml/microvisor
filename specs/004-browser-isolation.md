# Spec 004: Browser Isolation (Steel Browser)

## 1. Overview
This specification defines the requirements for running a headless Steel Browser instance inside a Firecracker microVM. This provides a secure sandbox for web-based agent tasks.

## 2. Base Image Requirements
The `rootfs` for browser workloads must include:
*   **Node.js (LTS):** Required to run the Steel Browser API layer.
*   **Chromium/Playwright Dependencies:** Standard libraries for headless browser execution (e.g., `libnss3`, `libatk1.0-0`, `libxcomposite1`, etc.).
*   **Steel SDK/Binary:** The core Steel Browser service.
*   **Tini/Init Script:** A robust init process to manage the browser and API server lifetimes.

## 3. Resource Allocation (Default)
Browser workloads are more resource-intensive than basic agents.
*   **CPU:** 2 vCPUs
*   **Memory:** 512 MB - 1024 MB
*   **Storage:** 2 GB rootfs (to accommodate browser binaries and cache).

## 4. Network Configuration
*   **Profile:** `browser`
*   **Inbound:** None.
*   **Outbound:** 
    *   TCP 80/443 (HTTP/S)
    *   UDP 53 (DNS)
    *   REJECT all private IP ranges (10.0.0.0/8, etc.)
    *   REJECT host gateway (172.16.0.1) except for DNS.

## 5. Mission Package (Metadata)
The metadata drive for a browser mission should include:
*   `STEEL_API_KEY`: For session management and anti-bot features.
*   `SESSION_ID`: A unique identifier for the session.
*   `TARGET_URL`: (Optional) An initial URL to load.
*   `PROXY_CONFIG`: (Optional) Custom proxy settings if not using Steel's defaults.

## 6. Lifecycle
1.  **Launch:** Orchestrator creates a unique metadata drive and starts the VM with the `browser` network profile.
2.  **Execution:** Steel Browser starts, listens for CDP or API commands from the agent (which may be internal or external).
3.  **Completion:** Upon task completion or timeout (default 30 mins), the orchestrator kills the Firecracker process and deletes the ephemeral metadata/rootfs.

## 7. Security Constraints
*   **No Persistence:** The browser's user data directory (UDD) must reside in memory (tmpfs) or be wiped between missions.
*   **Seccomp:** The browser should run with its internal sandbox enabled (if possible within the VM) and a restricted seccomp profile at the VM level.
