# ADR 007: Hierarchical State Machine Orchestration

## Status
Accepted

## Context
As we migrate from bash-based prototype scripts (`scripts/steel/`) to the production Rust control plane, the complexity of managing concurrent virtual machines, eBPF network hooks, storage snapshots, and hardware-level telemetry grows exponentially. 

Currently, the Rust orchestrator prototype (e.g., `Orchestrator::launch_vm`) uses a procedural, imperative approach: execute step A, then step B, then step C. If step C fails, rolling back steps A and B is brittle. Furthermore, this imperative style makes it difficult to attach clear, discrete observability spans (as mandated by **ADR 005**) to specific lifecycle phases, and almost impossible to reason about the exact state of a VM when a hardware contention event occurs.

We need a deterministic way to manage the lifecycle of thousands of concurrent microVMs and their associated resources.

## Decision
The Microvisor control plane will be architected as a **Hierarchical State Machine (State-Machine-of-State-Machines)**.

1. **Explicit State Representation:** Every major entity (VM, Network Interface, Storage Volume) must be modeled as a strict State Machine.
    *   *Example VM States:* `Allocated` -> `ProvisioningStorage` -> `AttachingNetwork` -> `Booting` -> `Running` -> `Terminating` -> `Destroyed`.
2. **Event-Driven Transitions:** Transitions between states must be triggered by discrete events (e.g., `StorageProvisionedEvent`, `KvmExitEvent`), not sequential function calls.
3. **Observability Hooks:** Every state transition must automatically emit a `tracing` span/event containing the 64-bit `session_id`, fulfilling **Constraint 008** for atomic lifecycle logging.
4. **Hierarchical Orchestration:** The parent `VmStateMachine` does not perform storage I/O directly. Instead, it dispatches a command to the `StorageStateMachine` and waits for an asynchronous event confirming the transition to `Ready`.

## Consequences

### Positive
*   **Deterministic Recovery:** If the system crashes or a sub-task fails, the orchestrator knows exactly which state the VM was in and can execute a precise rollback/cleanup transition.
*   **Observability Alignment:** State transitions map perfectly to `tokio-tracing` spans, giving us a clean, predictable timeline for every `session_id` without manually scattering log statements.
*   **Concurrency:** Modeling resources as independent state machines that communicate via events prevents deadlocks and makes the asynchronous `tokio` runtime highly efficient.

### Negative / Challenges
*   **High Boilerplate:** Implementing strict state machines in Rust (often utilizing the Typestate Pattern or enums with distinct states) requires significantly more upfront code than a simple procedural function.
*   **Complex Flow Control:** Developers cannot use simple `if/else` logic for lifecycle management; they must reason about asynchronous event loops and state handlers.

## Related Documents
*   `ADRs/005-control-plane-observability-stack.md`
*   `constraints/008-transparent-deep-observability.md`
