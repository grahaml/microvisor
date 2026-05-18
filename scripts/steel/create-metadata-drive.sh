#!/bin/bash
set -e

# create-metadata-drive.sh (Steel Browser)
# Creates an ephemeral ext4 image containing the Steel API key.

cd "$(dirname "$0")/../.."

STEEL_API_KEY="${1:-""}"
METADATA_FILE="resources/metadata.ext4"
TEMP_BUILD_DIR=$(mktemp -d)

# Ensure the temp dir is destroyed even on failure
trap 'rm -rf "$TEMP_BUILD_DIR"' EXIT

echo "Writing secrets to temporary build directory..."
if [ -n "$STEEL_API_KEY" ]; then
  echo "STEEL_API_KEY=$STEEL_API_KEY" > "$TEMP_BUILD_DIR/.env"
  chmod 600 "$TEMP_BUILD_DIR/.env"
else
  touch "$TEMP_BUILD_DIR/.env"
  echo "[!] No API Key provided. Running in Direct/Local mode."
fi

echo "Creating 2MB metadata image..."
dd if=/dev/zero of="$METADATA_FILE" bs=1M count=2 status=none

echo "Formatting image and injecting files (User-Space)..."
mkfs.ext4 -F -q -d "$TEMP_BUILD_DIR" "$METADATA_FILE"

# Restrict permissions
chmod 600 "$METADATA_FILE"

echo "Metadata drive created successfully."
