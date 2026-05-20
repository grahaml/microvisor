#!/usr/bin/env bash
# Tears down the Microvisor DM thin-pool and detaches its loop devices.
#
# By default the backing files are PRESERVED in $POOL_DATA_DIR so that
# re-running 'make pool' reattaches quickly without re-importing the 6 GB
# base image.
#
# Pass --purge to delete the backing files and start completely fresh.
#
# Required capabilities: runs as root (called via sudo from the Makefile).

set -euo pipefail

POOL_NAME="${POOL_NAME:-thin-pool-0}"
POOL_DATA_DIR="${POOL_DATA_DIR:-/var/lib/microvisor}"
PURGE="${1:-}"

POOL_DATA="$POOL_DATA_DIR/pool-data.img"
POOL_META="$POOL_DATA_DIR/pool-meta.img"
SENTINEL="resources/.pool-ready"

# ---------------------------------------------------------------------------
# Remove any active thin devices that are children of this pool
# (snapshots created by the orchestrator during testing)
# ---------------------------------------------------------------------------
while IFS= read -r dev; do
    if [ -n "$dev" ]; then
        echo "  Removing child device: $dev"
        dmsetup remove "$dev" 2>/dev/null || true
    fi
done < <(dmsetup deps -o blkdevname 2>/dev/null \
         | awk -F: -v pool="$POOL_NAME" '$2 ~ pool {print $1}' || true)

# ---------------------------------------------------------------------------
# Remove the pool device itself
# ---------------------------------------------------------------------------
if dmsetup info "$POOL_NAME" >/dev/null 2>&1; then
    echo "  Removing pool device /dev/mapper/$POOL_NAME ..."
    dmsetup remove "$POOL_NAME"
else
    echo "  Pool '$POOL_NAME' not active — skipping."
fi

# ---------------------------------------------------------------------------
# Detach loop devices
# ---------------------------------------------------------------------------
for img in "$POOL_DATA" "$POOL_META"; do
    if [ -f "$img" ]; then
        loop=$(losetup -j "$img" 2>/dev/null | cut -d: -f1)
        if [ -n "$loop" ]; then
            echo "  Detaching $loop  ($img)"
            losetup -d "$loop"
        fi
    fi
done

# ---------------------------------------------------------------------------
# Clear sentinel so 'make pool' knows setup is required
# ---------------------------------------------------------------------------
rm -f "$SENTINEL"

# ---------------------------------------------------------------------------
# Optional: delete backing files for a completely clean slate
# ---------------------------------------------------------------------------
if [ "$PURGE" = "--purge" ]; then
    echo "  --purge: deleting backing files in $POOL_DATA_DIR ..."
    rm -f "$POOL_DATA" "$POOL_META"
    echo "  Next 'make pool' will import the base image from scratch (~60 s)."
fi

echo "  Pool '$POOL_NAME' torn down."
