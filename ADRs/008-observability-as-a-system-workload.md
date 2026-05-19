# ADR 008: Observability as a System Workload

## Status
Proposed

## Context
The Microvisor control plane emits rich, structured telemetry via the `tracing` crate. To aggregate and visualize this telemetry without introducing external host-level dependencies (like Docker or system-wide daemons) and to maintain the bare-metal philosophy, we need an isolation-first strategy for the telemetry stack (OpenTelemetry Collector and Jaeger).

## Decision
We will treat the observability stack as a **Tier-0 System Workload**. Instead of running it on the host, it will be deployed inside a dedicated Firecracker microVM managed by the Microvisor control plane itself.

1.  **Dogfooding:** By running the observability stack inside our own virtualized infrastructure, we exercise the control plane's stability and performance before launching user workloads.
2.  **Strict Isolation:** The telemetry stack is confined by KVM, cgroups v2, and eBPF network fencing, ensuring that its resource usage (especially the Go-based OTel Collector) cannot impact host stability.
3.  **Static System IP:** The observability VM will be assigned a static IP (`10.0.0.2`) reserved via a dedicated path in the IPAM module, ensuring a deterministic destination for the control plane's OTLP exporter.
4.  **Ephemeral State:** The VM will utilize in-memory storage for Jaeger to protect the host disk and ensure a "clean slate" on reboot, aligning with the project's ephemeral state mandates.

## Consequences
- **Positive:** No Docker or non-standard daemons required on the host. Perfect architectural alignment. High fidelity testing of the orchestrator.
- **Negative:** Telemetry is lost on control plane reboot. Boot sequence is slightly longer as the system VM must initialize before traces can be received.
- **Neutral:** Increased initial complexity in the IPAM and orchestrator boot logic.
