# Architecture: The "Mini-Cloud" Orchestration Plane

## Overview
This document outlines the overarching architecture of our custom, bare-metal-like agent runtime. The system acts as a "mini-cloud," providing an API to dynamically provision, manage, and tear down highly isolated environments (microVMs) for executing untrusted AI workloads and headless browsers.

The architecture is strictly guided by our established constraints (Least Privilege, Strict Isolation, Ephemerality, Declarative Infrastructure, and Workload-Specific Egress).

## Core Components

### 1. The Rust Orchestrator (The Control Plane)
The brain of the system. A long-running Rust daemon operating on the host machine.
*   **Role:** Acts as the API server and lifecycle manager. It does *not* run as root (Constraint 000).
*   **Responsibilities:**
    *   **API Gateway:** Receives requests to launch workloads (e.g., "Run a Hermes agent," "Start a browser session").
    *   **Resource Allocation:** Selects an available IP address, MAC address, and generates unique TAP device names.
    *   **Image Provisioning:** Dynamically creates the `metadata.ext4` drive for secrets and ensures a pristine `rootfs.ext4` is available for the boot (Constraint 002).
    *   **Networking Setup:** Executes the necessary privileged commands (via `sudo` or a privileged helper binary) to create TAP devices and configure declarative `iptables`/`nftables` rules based on the requested Egress Profile (Constraint 004).
    *   **Firecracker Interfacing:** Launches the `firecracker` binary, configures it via the Unix Domain Socket REST API, and issues the `InstanceStart` command.
    *   **Monitoring & Teardown:** Monitors the VM's lifecycle and gracefully cleans up resources (killing the process, deleting TAP interfaces, wiping metadata drives) upon task completion or timeout.

### 2. Firecracker MicroVMs (The Compute Nodes)
The isolated execution environments.
*   **Role:** Provides hardware-level isolation for untrusted workloads using Linux KVM.
*   **Characteristics:**
    *   **Fast Boot:** Capable of booting into user space in <125ms.
    *   **Minimal Footprint:** Uses a stripped-down Linux kernel (`vmlinux`) and a highly specialized block device model (`virtio-blk`).
    *   **Strict Isolation:** Has its own kernel, process space, and memory. Cannot access host files or other VMs (Constraint 001).

### 3. Storage & State Management
Separating the OS from the data to maintain security and enable features like persistent browser sessions.
*   **The OS Drive (`/dev/vda`):** The `rootfs.ext4` image containing the base OS and required binaries (Hermes, Puppeteer). It is immutable/ephemeral per launch (Constraint 002).
*   **The Metadata Drive (`/dev/vdb`):** An orchestrator-generated `metadata.ext4` image injected at boot time containing ephemeral secrets (API keys) or startup arguments.
*   **The State Drive (`/dev/vdc` - Optional):** For workloads requiring state (like browser profiles), a persistent block device mapped to the user's specific session data.

### 4. Networking Topology (The Virtual Plumbing)
A strictly controlled network plane ensuring default-deny isolation.
*   **Host-Side (`tap` devices):** The Orchestrator provisions a dedicated `tap` interface for every microVM. 
*   **Routing:** The host acts as the router (`172.16.0.1`). VMs do not share a bridge unless explicitly required; traffic between `tap` devices is dropped by default to prevent lateral movement (Constraint 001).
*   **Egress Control:** The Orchestrator applies `iptables` rules on the host specifically for each `tap` interface based on its assigned profile (e.g., Profile A: Allow only Anthropic API; Profile B: Allow 80/443 but block SSRF metadata endpoints) (Constraint 004).

## Security Boundary Summary
1.  **Hardware Layer:** KVM ensures memory and CPU isolation.
2.  **OS Layer:** Ephemeral RootFS prevents persistent malware. Unprivileged agent execution (Constraint 000) limits damage if the agent breaks out of its immediate process.
3.  **Network Layer:** Host-level `iptables` rules restrict egress and lateral movement, regardless of what the guest OS attempts to do.
