#!/bin/bash
set -e

# latency-test.sh
# Measures boot-to-ready latency for Steel Browser microVMs.

DISTRO=${1:-"debian"}
API_SOCKET="/tmp/firecracker-bench.socket"
LOG_FILE="bench-results.log"

echo "[*] Starting Latency Benchmark for Distro: $DISTRO"

# 1. Start Firecracker
./bin/firecracker --api-sock $API_SOCKET &
FC_PID=$!

# 2. Record Start Time
START_TIME=$(date +%s%N)

# 3. Wait for API to be ready (Host side)
while [ ! -S $API_SOCKET ]; do sleep 0.01; done

# 4. Run Launch Configuration (Simplified for benchmark)
# TODO: Call scripts/steel/run-vm.sh with --distro flag once implemented

# 5. Poll for Guest API (Port 3000)
# We assume the VM IP is 172.16.0.2
echo "[*] Polling for Guest API..."
READY=0
while [ $READY -eq 0 ]; do
    if curl -s http://172.16.0.2:3000/health > /dev/null; then
        READY=1
    else
        sleep 0.05
    fi
    
    # Timeout after 10 seconds
    CURRENT_TIME=$(date +%s)
    if [ $((CURRENT_TIME - (START_TIME/1000000000))) -gt 10 ]; then
        echo "[-] Timeout waiting for Guest API."
        kill $FC_PID
        exit 1
    fi
done

# 6. Record End Time
END_TIME=$(date +%s%N)
LATENCY=$(( (END_TIME - START_TIME) / 1000000 ))

echo "[+] Success! $DISTRO Ready in ${LATENCY}ms"
echo "$(date): $DISTRO - ${LATENCY}ms" >> $LOG_FILE

# 7. Cleanup
kill $FC_PID
rm -f $API_SOCKET
