# Spec-005: Observable State Machine Orchestrator

## Overview
This specification defines the architecture of the Microvisor control plane as a high-performance, hierarchical state machine with integrated, zero-cost observability. It bridges the gap between high-level orchestration decisions and low-level kernel execution.

## 🎯 Design Goals
1. **Deterministic Lifecycle:** Every VM must follow a strict, event-driven state machine to ensure safe recovery and cleanup.
2. **Transparent Observability:** Telemetry is a first-class citizen, using 64-bit session IDs to correlate async Rust tasks with eBPF kernel events.
3. **Non-Blocking Execution:** Ensure the `tokio` runtime remains responsive by auditing all blocking "syscall-like" operations.

## 🏗️ State Machine Architecture

### 1. VM Lifecycle States
The VM lifecycle is managed by a `VmStateMachine` with the following discrete states:
*   `Pending`: Initial state, configuration validated.
*   `ProvisioningStorage`:Issuing `ioctl` for Device Mapper thin-provisioning snapshots.
*   `ConfiguringNetwork`: Creating TAP devices and loading eBPF NAT programs.
*   `InitializingCgroup`: Setting up cgroup v2 resource limits and pinning.
*   `LaunchingVMM`: Executing the hypervisor process (e.g., Firecracker).
*   `Running`: VM is active and being monitored.
*   `Terminating`: Graceful shutdown or emergency teardown initiated.
*   `Destroyed`: All resources (IPs, snapshots, cgroups) have been released.

### 2. Implementation Pattern
*   **Enums as States:** Use a Rust `enum` to represent mutually exclusive states.
*   **Event-Driven Transitions:** Transitions are triggered by `Result<SuccessEvent, ErrorEvent>` from sub-components (Storage, Network).
*   **Atomic Transitions:** No state transition is complete without emitting a corresponding `tracing` event.

## 📊 Observability Integration

### 1. Session Traceability (ADR 003/005)
*   Every orchestration session generates a unique `u64` **`session_id`**.
*   This ID is injected into the root `tracing::Span` of the `launch_vm` task.
*   All child spans and log events automatically inherit this ID via task-local storage.

### 2. Syscall Auditing (ADR 005)
*   Critical boundaries (e.g., writing to `/sys/fs/cgroup/`, creating TAP devices) must be wrapped in a timing span.
*   **Primary defense — wrap, don't just measure:** All known-blocking calls must be executed via `tokio::task::spawn_blocking` (multi-threaded runtime) or `tokio::task::block_in_place` (within a `current_thread` runtime). This includes, at minimum:
    - Device-mapper `ioctl`s on `/dev/mapper/control` and on pool devices
    - Reads/writes to `/sys/fs/cgroup/...`
    - `sched_setaffinity` / `sched_setscheduler` calls
    - eBPF map updates that may serialize under verifier locks
    - TAP device creation via `ioctl(TUNSETIFF)`

    Instrumenting these without wrapping them is insufficient — by the time the warning fires, the reactor is already starved.
*   **Guardrail — measure, don't only wrap:** Any blocking operation exceeding 1ms inside a `spawn_blocking` worker should be logged as a warning so that pathological cases (e.g., a runaway thin-pool extension) are still visible. Additionally enable `tokio::runtime::Builder::enable_metrics_poll_count_histogram()` and alert on slow-poll counts at the runtime level. The 1ms threshold is the floor for **detection**, not the upper bound of acceptable behavior.

### 3. Always-On Binary Telemetry (ADR 003/004)
*   The orchestrator will maintain a BPF Hash Map (`vm_session_map`) linking the guest `cgroup_id` to the `session_id`.
*   During state transitions to `Running`, the orchestrator writes this mapping to the kernel to enable hardware-level correlation (ADR 006).

## 🛠️ Implementation Requirements

### 1. Crates
*   `tracing`: Core facade for instrumentation.
*   `tracing-subscriber`: Collector for spans (configured for structured output).
*   `rand`: Generation of high-entropy 64-bit `session_id`s.

### 2. Function Decoration
*   All major lifecycle functions must use `#[instrument(skip(self), fields(session_id = %config.session_id))]`.

### 3. Error Handling
*   Errors must be "Rich Errors" carrying the `session_id` and the state at which the failure occurred to enable precise debugging of the state machine.

## Related Documents
*   `ADRs/003-observability-driven-architecture.md`
*   `ADRs/005-control-plane-observability-stack.md`
*   `ADRs/007-hierarchical-state-machines.md`
*   `constraints/008-transparent-deep-observability.md`
