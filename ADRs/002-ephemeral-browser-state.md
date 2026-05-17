# ADR 002: Ephemeral Root Filesystems for Browser Sessions

## Status
Proposed

## Context
Browser sessions generate a significant amount of state:
1.  **Cookies and Local Storage:** Can be used to track agents across different tasks.
2.  **Browser Cache:** Can contain sensitive information from previous sessions.
3.  **Malware Persistence:** If a browser is compromised, malware may attempt to hide in the profile directory.

For high-security AI agent operations, we need to ensure that Session A cannot influence Session B.

## Decision
We will use strictly ephemeral root filesystems for all browser-based microVMs.

### Key Implementation Details:
*   **Copy-on-Write (CoW):** The "Gold Image" (rootfs.ext4) will be treated as read-only. For each session, the orchestrator will create a thin-provisioned clone or a simple `cp` of the image to a temporary location.
*   **Mandatory Deletion:** The temporary rootfs image will be deleted immediately upon VM termination.
*   **In-Memory User Data:** The browser's `user-data-dir` will be mapped to a `tmpfs` (RAM disk) within the guest if memory permits, or simply reside on the ephemeral rootfs.

## Consequences
### Positive:
*   **Perfect Privacy:** No tracking artifacts survive between sessions.
*   **Security Baseline:** Malware cannot persist beyond a single task.
*   **Deterministic Behavior:** Every session starts with the exact same browser configuration.

### Negative:
*   **Performance Hit:** No caching means every page load starts from scratch (no local cache). This is partially mitigated by using Steel's residential proxies and high-speed host networking.
*   **Disk I/O:** Repeatedly copying/creating 2GB image files can stress host storage. We will investigate using "Snapshot/Overlay" filesystems (like Btrfs or thin-pool LVM) to optimize this.

## Alternatives Considered
*   **Persistent Profiles:** Rejected due to privacy and security risks.
*   **Selective Wiping:** Rejected as it is complex and error-prone (hard to guarantee everything is wiped).
