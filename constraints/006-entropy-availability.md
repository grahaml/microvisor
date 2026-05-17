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
*   **Guest Check:** Run `cat /proc/sys/kernel/random/entropy_avail` inside the guest. It should consistently show values above 3000.
*   **Performance:** Verify that HTTPS requests from within the VM do not experience "first-call" latency spikes during the TLS handshake.
