# ADR 001: Isolated Browser Infrastructure

## Status
Proposed

## Context
AI agents often require web browsing capabilities (e.g., using Steel Browser) to perform research, interact with web-based tools, or scrape data. Browsing the live web introduces significant security risks, including:
1.  **Remote Code Execution (RCE):** Malicious websites exploiting browser vulnerabilities.
2.  **Tracking & Fingerprinting:** Sites identifying the agent and potentially blocking it or feeding it manipulated data.
3.  **Credential Theft:** Cross-site scripting (XSS) or other attacks targeting the agent's session data.

Traditional container-based isolation (Docker) shares the host kernel, which presents a larger attack surface.

## Decision
We will use Firecracker microVMs to provide "Layer 1" hardware-enforced isolation for all browser-based workloads. Specifically, each Steel Browser session or instance will run within its own microVM.

### Key Implementation Details:
*   **Kernel Separation:** Each VM runs its own Linux kernel, preventing a browser exploit from accessing the host kernel directly.
*   **Ephemeral State:** VMs will be destroyed immediately after the task is complete, ensuring no cross-session data leakage.
*   **Network Profiling:** Browser VMs will use the "browser" network profile, which allows egress to 80/443 but restricts access to internal host resources and private IP ranges.
*   **Resource Fencing:** Strict memory and CPU limits will be enforced at the Firecracker level to prevent resource exhaustion (DoS) by a compromised browser.

## Consequences
### Positive:
*   **Enhanced Security:** Hardware-level isolation significantly reduces the risk of a browser-based escape affecting the host system.
*   **Clean Slate:** Every session starts with a pristine rootfs, eliminating persistent malware or tracking artifacts.
*   **Compliance:** Easier to audit and satisfy security requirements for running untrusted code/browsing.

### Negative:
*   **Resource Overhead:** MicroVMs have slightly higher memory overhead (~20-50MB) compared to native containers.
*   **Boot Latency:** While fast (~150ms), microVM boot times are still slower than starting a process in an existing container.
*   **Complexity:** Managing VM lifecycles and networking is more complex than standard Docker orchestration.

## Alternatives Considered
*   **Standard Docker Containers:** Rejected due to shared kernel risk.
*   **gVisor:** A strong middle ground, but Firecracker's KVM-based isolation is considered more robust for high-risk web browsing.
*   **Browser-in-the-Cloud (External):** Rejected for latency and lack of control over the "Mission Package" (secrets, environment).
