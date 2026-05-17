# Constraint: Strict Isolation and Containment

## Definition
Every microVM must be a completely sealed environment. There must be zero trust between VMs, and zero trust between a VM and the host. 

## What We Do NOT Do
*   **No Shared State:** VMs must never share block devices, memory space, or local filesystems.
*   **No Lateral Movement:** A compromised VM must not be able to communicate with another VM. We do not implement a "mesh" network between agents unless it is a specific, explicitly defined requirement mediated by a secure router.
*   **No Host Leakage:** The guest VM must have no visibility into the host OS. It cannot know it is a VM, cannot access host metrics, and cannot access the orchestrator's API or data.
*   **No Noisy Neighbors:** We must prevent resource starvation. A single runaway agent must not be able to consume 100% of the host CPU or I/O, impacting other VMs.

## Implementation Guide
*   Rely entirely on Firecracker's KVM-based isolation.
*   Implement `cgroups` or Firecracker's built-in rate limiters for CPU and I/O throttling.
*   Host networking (`iptables`) must strictly drop traffic attempting to route from one `tap` interface to another.
