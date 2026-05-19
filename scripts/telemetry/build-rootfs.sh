#!/bin/bash
set -e

# Configuration
ALPINE_VERSION="3.20"
ROOTFS_SIZE_MB=512
ROOTFS_IMAGE="resources/telemetry-rootfs.ext4"
OTEL_COLLECTOR_VERSION="0.114.0"
JAEGER_VERSION="1.62.0"

echo "Building Telemetry RootFS..."

# 1. Create a sparse file for the rootfs
mkdir -p resources
fallocate -l "${ROOTFS_SIZE_MB}M" "$ROOTFS_IMAGE"
mkfs.ext4 "$ROOTFS_IMAGE"

# 2. Mount the rootfs
MOUNT_DIR=$(mktemp -d)
sudo mount "$ROOTFS_IMAGE" "$MOUNT_DIR"

trap "sudo umount $MOUNT_DIR && rm -rf $MOUNT_DIR" EXIT

# 3. Bootstrap Alpine
sudo apk add --no-root --initdb --add alpine-base --repository "http://dl-cdn.alpinelinux.org/alpine/v${ALPINE_VERSION}/main" --root "$MOUNT_DIR"

# 4. Download OTel Collector & Jaeger
# In a real environment, we'd pull specific architectures.
OTEL_URL="https://github.com/open-telemetry/opentelemetry-collector-releases/releases/download/v${OTEL_COLLECTOR_VERSION}/otelcol-contrib_${OTEL_COLLECTOR_VERSION}_linux_amd64.tar.gz"
JAEGER_URL="https://github.com/jaegertracing/jaeger/releases/download/v${JAEGER_VERSION}/jaeger-${JAEGER_VERSION}-linux-amd64.tar.gz"

echo "Downloading OTel Collector..."
curl -L "$OTEL_URL" | sudo tar -xz -C "$MOUNT_DIR/usr/local/bin" otelcol-contrib

echo "Downloading Jaeger..."
curl -L "$JAEGER_URL" | sudo tar -xz -C "$MOUNT_DIR/usr/local/bin" --strip-components=1

# 5. Create Init Script
cat <<EOF | sudo tee "$MOUNT_DIR/init" > /dev/null
#!/bin/sh
mount -t proc proc /proc
mount -t sysfs sys /sys
ip link set lo up
ip addr add 10.0.0.2/24 dev eth0
ip link set eth0 up
ip route add default via 10.0.0.1

echo "Starting Jaeger..."
export MEMORY_MAX_TRACES=10000
/usr/local/bin/jaeger-all-in-one --collector.otlp.enabled=true &

echo "Starting OTel Collector..."
/usr/local/bin/otelcol-contrib --config=/etc/otel-collector-config.yaml &

# Keep VM alive
exec /bin/sh
EOF
sudo chmod +x "$MOUNT_DIR/init"

# 6. OTel Collector Config
cat <<EOF | sudo tee "$MOUNT_DIR/etc/otel-collector-config.yaml" > /dev/null
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

exporters:
  otlp:
    endpoint: localhost:4317
    tls:
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [otlp]
EOF

echo "Telemetry RootFS build complete: $ROOTFS_IMAGE"
