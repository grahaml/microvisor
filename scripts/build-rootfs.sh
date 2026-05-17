#!/bin/bash
set -e

IMAGE_NAME="hermes-firecracker"
ROOTFS_FILE="resources/rootfs.ext4"
SIZE_MB=2048

echo "[1/4] Ensuring resources directory exists..."
mkdir -p resources

echo "[2/4] Building Docker image..."
docker build -t $IMAGE_NAME .

echo "[3/4] Creating empty ext4 image ($SIZE_MB MB)..."
dd if=/dev/zero of=$ROOTFS_FILE bs=1M count=$SIZE_MB
mkfs.ext4 -F $ROOTFS_FILE

echo "[4/4] Exporting filesystem from Docker to ext4..."
MOUNT_DIR=$(mktemp -d)
ID=$(docker create $IMAGE_NAME)

# Mount the ext4 file (requires sudo)
sudo mount $ROOTFS_FILE $MOUNT_DIR

# Export the container's filesystem and extract it into the mounted ext4 file
docker export $ID | sudo tar -x -C $MOUNT_DIR

# Unmount and cleanup
sudo umount $MOUNT_DIR
rmdir $MOUNT_DIR
docker rm $ID

echo "Success! RootFS built at $ROOTFS_FILE"
