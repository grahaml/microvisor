# Constraint: Principle of Least Privilege

## Definition
The system must operate using the absolute minimum permissions necessary to function. No component should have overarching "root" or "admin" access by default.

## What We Do NOT Do
*   **No Root Orchestrator:** The Rust orchestration plane must not run as the host machine's `root` user. It must run under a dedicated, unprivileged service account.
*   **No Root Agents:** Agents inside the microVMs must not run as the `root` user of the guest OS. They must execute as a restricted user (e.g., `agent_user`).
*   **No Broad Capabilities:** Do not grant broad Linux capabilities (e.g., `CAP_SYS_ADMIN`) to any process unless explicitly justified, documented, and narrowly scoped.

## Implementation Guide
*   Use Linux groups and ACLs to manage access to the `/dev/kvm` device and TAP network interfaces.
*   The guest `init.sh` must drop privileges using tools like `su` or `runuser` before executing the agent payload.
