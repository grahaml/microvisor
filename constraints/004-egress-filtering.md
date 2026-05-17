# Constraint: Workload-Specific Egress Filtering

## Definition
MicroVMs must operate with the most restrictive network egress policy possible for their specific workload. Networking must be declarative and profile-based, never uniformly "open" across the cluster.

## What We Do NOT Do
*   **No Unmonitored Traffic:** We do not allow outbound traffic to leave the host without knowing exactly which agent sent it, what its profile allows, and logging the destination.
*   **No Blanket Open Internet:** While some workloads require broad access, the cluster as a whole must default to a "Default Deny" policy. Open access must be explicitly requested and granted per-VM.
*   **No Unbound DNS for API Agents:** Agents designated for specific API tasks (e.g., calling Anthropic) must only be able to resolve and connect to those specific hostnames.

## Egress Profiles & Implementation Guide

### Profile A: Strict API-Bound (Default Deny)
*   **Use Case:** Backend agents, code executors, or orchestrators (like a strict Hermes node).
*   **Implementation:** The host `iptables`/`nftables` configuration drops all outbound packets from this VM's `tap` interface. The Rust orchestrator dynamically adds explicit `ALLOW` rules *only* for necessary endpoints (e.g., `api.anthropic.com` on port 443).

### Profile B: Open Browser (Proxy Monitored)
*   **Use Case:** Headless browsers (like Puppeteer/Playwright instances via Steel.dev) navigating the public web.
*   **Implementation:** The VM is permitted to access arbitrary port 80/443 traffic. However, this traffic should ideally be routed through a transparent proxy or DNS sinkhole on the host. This allows the orchestration plane to:
    1.  Log all visited domains for security auditing.
    2.  Block known malicious domains, botnet command-and-control servers, or internal IP ranges (e.g., `169.254.169.254` AWS metadata endpoints, or `10.0.0.0/8` local networks) to prevent Server-Side Request Forgery (SSRF) attacks against our own infrastructure.
