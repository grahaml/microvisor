# Constraint: Ephemerality and Immutability

## Definition
The foundational operating system of the microVM must be treated as read-only or entirely disposable. Agents cannot permanently modify their core environment, though explicitly defined user state (like browser profiles) may be persisted.

## What We Do NOT Do
*   **No Persistent RootFS Modifications:** Agents must not be allowed to permanently install software or change system configurations on the main system drive. The OS layer is strictly disposable.
*   **No State Rescue of the OS:** We do not "rescue" or "repair" broken OS environments. If an agent crashes or corrupts its OS, the VM is destroyed.
*   **No Unmanaged State Persistence:** State must not persist "by accident." Any state that needs to survive across sessions (like browser cookies or local storage) must be explicitly managed and separated from the root OS.
*   **No Long-Lived Instances:** MicroVMs are not pets. They are spun up for a task (or a user session) and destroyed when the session completes or times out.

## Implementation Guide
*   The base `rootfs.ext4` should be mounted as read-only by Firecracker, or a fresh copy must be used for every single VM launch.
*   **Targeted State Persistence:** To support features like persistent browser sessions across ephemeral VMs, specific directories (e.g., `/home/agent_user/.config/google-chrome`) must be mapped to a dedicated, persistent secondary block device, or the microVM's state must be paused and persisted using Firecracker's `SnapshotCreate` API.
*   Scratch space for transient downloads must be a volatile `tmpfs` or a disposable secondary drive securely wiped upon termination.
