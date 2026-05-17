#!/bin/bash
set -e

# Hardened Metadata Drive Creation
cd "$(dirname "$0")/.."

if [ -z "$1" ]; then
  echo "Error: API Key must be provided as the first argument." >&2
  exit 1
fi

API_KEY="$1"
METADATA_FILE="resources/metadata.ext4"
TEMP_BUILD_DIR=$(mktemp -d)

# Ensure the temp dir is destroyed even on failure
trap 'rm -rf "$TEMP_BUILD_DIR"' EXIT

echo "Writing secrets to temporary build directory..."
echo "ANTHROPIC_API_KEY=$API_KEY" > "$TEMP_BUILD_DIR/.env"
# Only the owner of the process (you) can read this temp file
chmod 600 "$TEMP_BUILD_DIR/.env"

echo "Creating 2MB metadata image..."
dd if=/dev/zero of="$METADATA_FILE" bs=1M count=2 status=none

echo "Formatting image and injecting files (User-Space)..."
mkfs.ext4 -F -q -d "$TEMP_BUILD_DIR" "$METADATA_FILE"

# Restrict permissions on the generated image file itself
# So only the user running Firecracker can open it
chmod 600 "$METADATA_FILE"

echo "Metadata drive created successfully and hardened."
