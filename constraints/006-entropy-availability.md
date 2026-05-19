# Constraint: Entropy Availability

## Definition
The microVM environment must provide a high-quality source of entropy to the guest OS. This is particularly critical for browser-based workloads (e.g., Steel Browser) which rely heavily on cryptographic operations for TLS/SSL handshakes, secure cookies, and anti-fingerprinting.

## What We Do NOT Do
*   **No Starved Entropy:** We do not rely solely on the guest kernel's internal entropy gathering (which is often limited in headless/disk-less microVMs).
*   **No Blocking Calls:** Guest processes should not hang waiting for `/dev/random` to refill.

## Implementation Guide
*   **virtio-rng:** Every browser-enabled microVM must be configured with a `virtio-rng` device via the Firecracker API.
*   **Host Source:** The `virtio-rng` device should be backed by the host's `/dev/urandom` or a hardware RNG if available.
*   **Kernel Config:** The guest kernel (`vmlinux.bin`) must have `CONFIG_HW_RANDOM_VIRTIO` enabled to recognize the device.

## Validation
*   **Guest Check (kernel-init signal):** Confirm `random: crng init done` appears in the guest kernel log (`dmesg`) before user-space starts. On any modern kernel (≥ 5.18, post Jason Donenfeld's RNG overhaul), this is the authoritative signal that the RNG is seeded and non-blocking. Note: `/proc/sys/kernel/random/entropy_avail` is **not** a valid validation signal on modern kernels — it is capped at 256 by design and no longer represents pool depth.
*   **Performance:** Measure HTTPS handshake p99 from within the guest. The presence of `virtio-rng` should eliminate "first-call" latency spikes during TLS handshakes; that latency, not a synthetic entropy reading, is the metric that matters.
