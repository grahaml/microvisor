# Constraint: Declarative Infrastructure (No Manual Patching)

## Definition
The entire orchestration plane, VM configurations, and networking rules must be defined entirely in code.

## What We Do NOT Do
*   **No "Hot-Patching":** We do not SSH into the host or use manual `iptables` commands to fix networking issues on the fly. 
*   **No Mutable Config Files:** The Rust orchestrator must not rely on hand-edited configuration files that drift over time.
*   **No "Snowflake" Environments:** Every VM launch must be identical and deterministic based on the provided parameters.

## Implementation Guide
*   All infrastructure state (networking rules, routing tables) must be asserted by the Rust orchestration plane on startup.
*   If a change to the environment is required (e.g., adding a new allowed API endpoint), the code/spec must be updated, compiled, and the orchestrator restarted to apply the new declarative state.
