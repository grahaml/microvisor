#!/usr/bin/env bash
# Sets up a Device Mapper thin-provisioning pool for Microvisor on a dev machine.
#
# Backing storage is two sparse files under $POOL_DATA_DIR (default:
# /var/lib/microvisor/). No real disk partition is required.
#
# Idempotent behaviour:
#   - Pool already active → exit 0 immediately (nothing to do).
#   - Backing files exist but pool is inactive (e.g. after reboot) →
#     re-attach loop devices and re-create the DM device; skip the slow
#     base-image import because the data is already in the pool files.
#   - Backing files don't exist → full setup including base-image import.
#
# Required capabilities: runs as root (called via sudo from the Makefile).

set -euo pipefail

POOL_NAME="${POOL_NAME:-thin-pool-0}"
BASE_IMAGE="${BASE_IMAGE:-resources/browser-rootfs.ext4}"
POOL_DATA_DIR="${POOL_DATA_DIR:-/var/lib/microvisor}"
POOL_SIZE="${POOL_SIZE:-20G}"

POOL_DATA="$POOL_DATA_DIR/pool-data.img"
POOL_META="$POOL_DATA_DIR/pool-meta.img"
SENTINEL="resources/.pool-ready"

# ---------------------------------------------------------------------------
# Guard: already active
# ---------------------------------------------------------------------------
if dmsetup info "$POOL_NAME" >/dev/null 2>&1; then
    echo "  Pool '$POOL_NAME' is already active — nothing to do."
    touch "$SENTINEL"
    exit 0
fi

# ---------------------------------------------------------------------------
# Guard: required inputs
# ---------------------------------------------------------------------------
if [ ! -f "$BASE_IMAGE" ]; then
    echo "ERROR: base image not found: $BASE_IMAGE" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Step 1 — Create backing files (first time only)
# ---------------------------------------------------------------------------
FIRST_TIME=false
if [ ! -f "$POOL_DATA" ] || [ ! -f "$POOL_META" ]; then
    FIRST_TIME=true
fi

mkdir -p "$POOL_DATA_DIR"

if $FIRST_TIME; then
    echo "  Creating pool backing files in $POOL_DATA_DIR ..."
    echo "    pool-data.img : $POOL_SIZE (sparse — no disk space consumed until written)"
    echo "    pool-meta.img : 200M"
    truncate -s "$POOL_SIZE" "$POOL_DATA"
    truncate -s 200M         "$POOL_META"

    # DM thin-pool requires the metadata device to be zeroed on first use.
    echo "  Zeroing metadata device (one-time, ~200 MB) ..."
    dd if=/dev/zero of="$POOL_META" bs=1M status=progress conv=fsync
fi

# ---------------------------------------------------------------------------
# Step 2 — Attach backing files as loop devices
# ---------------------------------------------------------------------------
echo "  Attaching loop devices ..."
LOOP_DATA=$(losetup -f --show "$POOL_DATA")
LOOP_META=$(losetup -f --show "$POOL_META")
echo "    data : $LOOP_DATA  ($POOL_DATA)"
echo "    meta : $LOOP_META  ($POOL_META)"

# ---------------------------------------------------------------------------
# Step 3 — Create the thin-pool DM device
#
# Table parameters:
#   128        chunk size in 512-byte sectors = 64 KB (granularity of CoW)
#   32768      low-water-mark in sectors = 16 MB free; triggers events
#   1          number of feature args that follow
#   skip_block_zeroing   don't zero new chunks (faster; fine for ephemeral VMs)
#   error_if_no_space    return I/O errors when full (ADR-010: loud failure)
# ---------------------------------------------------------------------------
echo "  Creating thin-pool device /dev/mapper/$POOL_NAME ..."
DATA_SECTORS=$(blockdev --getsz "$LOOP_DATA")
dmsetup create "$POOL_NAME" --table \
    "0 $DATA_SECTORS thin-pool $LOOP_META $LOOP_DATA 128 32768 1 skip_block_zeroing error_if_no_space"

# ---------------------------------------------------------------------------
# Step 4 — Import base rootfs image as thin volume #1 (first time only)
#
# The Microvisor storage manager always creates VM snapshots from volume #1.
# This step writes the full base image into the pool once; subsequent VM
# launches just create a CoW snapshot of this volume (fast, zero-copy).
# ---------------------------------------------------------------------------
if $FIRST_TIME; then
    IMAGE_SIZE=$(stat -c%s "$BASE_IMAGE")
    IMAGE_SECTORS=$(( IMAGE_SIZE / 512 ))

    echo "  Importing base image as thin volume #1 ..."
    echo "    source : $BASE_IMAGE  ($(( IMAGE_SIZE / 1024 / 1024 / 1024 )) GB)"
    echo "    This is a one-time operation and takes ~60 seconds for a 6 GB image."

    # Allocate volume slot #1 in the pool metadata.
    dmsetup message "$POOL_NAME" 0 "create_thin 1"

    # Create a temporary device node so we can write to the volume.
    dmsetup create base-image-1 --notable
    dmsetup load   base-image-1 --table \
        "0 $IMAGE_SECTORS thin /dev/mapper/$POOL_NAME 1"
    dmsetup resume base-image-1

    # Raw block copy — identical to writing a disk image to a USB drive.
    dd if="$BASE_IMAGE" of=/dev/mapper/base-image-1 bs=4M status=progress
    sync

    # Remove the temporary device node (the data is now permanently in the pool).
    dmsetup remove base-image-1

    echo "  Base image imported as volume #1."
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------
touch "$SENTINEL"
echo ""
echo "  Pool '$POOL_NAME' ready at /dev/mapper/$POOL_NAME"
echo "  Run 'make check' to verify all prerequisites."
