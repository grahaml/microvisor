# Firecracker Hermes Infrastructure

A high-security, bare-metal-like isolation layer for AI agents using Firecracker microVMs. This project replaces k3s-based sandboxing with dedicated kernels and hardware-enforced isolation.

## Background & Motivation
This project serves as the foundation for a custom agent infrastructure. We have moved away from k3s/Kubernetes to leverage Firecracker microVMs for "Layer 1" isolation. This allows us to spin up autonomous agents (like Hermes) with their own dedicated kernels and filesystems, providing bare-metal performance with hardware-enforced security.

## Project Structure
- `bin/`: Contains the Firecracker binary (git-ignored).
- `resources/`: Contains the guest kernel, rootfs images, and metadata drives (git-ignored).
- `scripts/`: Automation scripts for building images and launching VMs.

## Setup Progress
- [x] Phase 1: Workspace & Tooling Setup
- [ ] Phase 2: Custom RootFS via Docker
- [ ] Phase 3: Secure Metadata Drive
- [ ] Phase 4: Host Network Configuration
- [ ] Phase 5: Firecracker Launch Script

## Prerequisites
- Linux Host (Ubuntu 24.04 recommended)
- KVM support enabled
- Docker (for building the rootfs)
- Python 3 & pip (inside the guest VM)
