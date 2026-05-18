# Microvisor Orchestrator

> **AI Agents:** Please read [AGENTS.md](./AGENTS.md) for core security mandates and architectural patterns before making changes.

A high-security, bare-metal-like isolation layer for AI agents using Firecracker microVMs. This project replaces k3s-based sandboxing with dedicated kernels and hardware-enforced isolation.

## Background & Motivation
This project serves as the foundation for a custom agent infrastructure. We have moved away from k3s/Kubernetes to leverage Firecracker microVMs for "Layer 1" isolation. This allows us to spin up autonomous agents (like Hermes) with their own dedicated kernels and filesystems, providing bare-metal performance with hardware-enforced security.

## Project Structure
- `bin/`: Contains the Firecracker binary (git-ignored).
- `resources/`: Contains the guest kernel, rootfs images, and metadata drives (git-ignored).
- `scripts/hermes/`: Automation for the Hermes AI agent (Docker-based).
- `scripts/steel/`: Automation for Steel Browser isolation (Debootstrap-based).
- `ADRs/`: Architecture Decision Records for the Microvisor orchestrator.
- `specs/`: Detailed technical specifications for every component.
- `constraints/`: Enforced security and resource fencing rules.

## Setup Progress
- [x] Phase 1: Workspace & Tooling Setup
- [x] Phase 2: Custom RootFS (Hermes & Steel)
- [x] Phase 3: Secure Metadata Injection
- [x] Phase 4: Host Network Configuration
- [x] Phase 5: Firecracker Launch Automation (Multi-Profile)

## Prerequisites
- Linux Host (Ubuntu 24.04 recommended)
- KVM support enabled
- `debootstrap` (for Steel Browser image)
- Docker (for Hermes image)
- Python 3 & Node.js (inside respective guest VMs)
