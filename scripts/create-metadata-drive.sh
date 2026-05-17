#!/bin/bash
set -e

cd "$(dirname "$0")/.."

if [ -z "$1" ]; then
  echo "Error: API Key must be provided as the first argument." >&2
  exit 1
fi

API_KEY="$1"
METADATA_FILE="resources/metadata.ext4"
TEMP_BUILD_DIR=$(mktemp -d)

echo "Ensuring resources directory exists..."
mkdir -p resources

echo "Writing secrets to temporary build directory..."
echo "ANTHROPIC_API_KEY=$API_KEY" > "$TEMP_BUILD_DIR/.env"

echo "Creating 2MB metadata image at $METADATA_FILE..."
dd if=/dev/zero of="$METADATA_FILE" bs=1M count=2 status=none

echo "Formatting image as ext4 and injecting files (User-Space)..."
# The -d flag tells mkfs to copy the contents of the directory into the image
mkfs.ext4 -F -q -d "$TEMP_BUILD_DIR" "$METADATA_FILE"

echo "Cleaning up temporary files..."
rm -rf "$TEMP_BUILD_DIR"

echo "Metadata drive created successfully at $METADATA_FILE"
