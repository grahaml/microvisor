#!/bin/bash
set -e

cd "$(dirname "$0")/.."

if [ -z "$1" ]; then
  echo "Error: API Key must be provided as the first argument." >&2
  exit 1
fi

API_KEY="$1"
METADATA_FILE="resources/metadata.ext4"
MOUNT_DIR=$(mktemp -d)

echo "Ensuring resources directory exists..."
mkdir -p resources

echo "Creating 2MB metadata image at $METADATA_FILE..."
dd if=/dev/zero of="$METADATA_FILE" bs=1M count=2 status=none

echo "Formatting image as ext4..."
mkfs.ext4 -F -q "$METADATA_FILE"

echo "Mounting image to $MOUNT_DIR..."
sudo mount -o loop "$METADATA_FILE" "$MOUNT_DIR"

echo "Writing secrets to .env file..."
echo "ANTHROPIC_API_KEY=$API_KEY" | sudo tee "$MOUNT_DIR/.env" > /dev/null

echo "Unmounting and cleaning up..."
sudo umount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"

echo "Metadata drive created successfully at $METADATA_FILE"
