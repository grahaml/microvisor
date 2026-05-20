#!/usr/bin/env bash
# Sets up a Device Mapper thin-provisioning pool for Microvisor on a dev machine.
#
# Backing storage is two sparse files under $POOL_DATA_DIR (default:
# /var/lib/microvisor/). No real disk partition is required.
#
# Idempotent behaviour:
#   - Pool already active → exit 0 immediately.
#   - Backing files exist but pool inactive (e.g. after reboot) →
#     re-attach loop devices and re-create the DM device; skip the base-image
#     import because the data is already in the pool files.
#   - Backing files don't exist → full setup including base-image import.
#
# Disk space required: ~6 GB for the base image import (one time).
# The sparse pool-data.img and pool-meta.img files consume no space until written.
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
# Step 1 — Create sparse backing files (first time only)
#
# truncate creates sparse files: they appear to be the specified size but
# consume no real disk space until blocks are written. They also read as
# all-zeros, which is exactly what DM thin-pool needs for a fresh metadata
# device — no explicit dd zeroing step is required or safe (dd without a
# count= limit would fill the entire disk).
# ---------------------------------------------------------------------------
FIRST_TIME=false
if [ ! -f "$POOL_DATA" ] || [ ! -f "$POOL_META" ]; then
    FIRST_TIME=true
fi

mkdir -p "$POOL_DATA_DIR"

if $FIRST_TIME; then
    echo "  Creating pool backing files in $POOL_DATA_DIR ..."
    echo "    pool-data.img : $POOL_SIZE (sparse)"
    echo "    pool-meta.img : 200M  (sparse)"
    truncate -s "$POOL_SIZE" "$POOL_DATA"
    truncate -s 200M         "$POOL_META"
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
#   128        chunk size in 512-byte sectors = 64 KB (CoW granularity)
#   32768      low-water-mark in sectors = 16 MB free; triggers udev events
#   1          number of feature args that follow
#   skip_block_zeroing   don't zero new chunks (fine for ephemeral VMs)
#   error_if_no_space    return I/O errors when full (ADR-010: loud failure)
# ---------------------------------------------------------------------------
echo "  Creating thin-pool device /dev/mapper/$POOL_NAME ..."
DATA_SECTORS=$(blockdev --getsz "$LOOP_DATA")
dmsetup create "$POOL_NAME" --table \
    "0 $DATA_SECTORS thin-pool $LOOP_META $LOOP_DATA 128 32768 1 skip_block_zeroing error_if_no_space"

# ---------------------------------------------------------------------------
# Step 4 — Import base rootfs image as thin volume #1 (first time only)
#
# The storage manager always snapshots from volume #1. This writes the full
# base image into the pool once; each VM launch then creates a CoW snapshot
# of this volume — fast and zero-copy.
# ---------------------------------------------------------------------------
if $FIRST_TIME; then
    IMAGE_SIZE=$(stat -c%s "$BASE_IMAGE")
    IMAGE_SECTORS=$(( IMAGE_SIZE / 512 ))

    echo "  Importing base image as thin volume #1 ..."
    echo "    source  : $BASE_IMAGE  ($(( IMAGE_SIZE / 1024 / 1024 / 1024 )) GB)"
    echo "    sectors : $IMAGE_SECTORS"
    echo "    This writes ~$(( IMAGE_SIZE / 1024 / 1024 / 1024 )) GB and takes ~60 seconds."

    dmsetup message "$POOL_NAME" 0 "create_thin 1"

    dmsetup create base-image-1 --notable
    dmsetup load   base-image-1 --table \
        "0 $IMAGE_SECTORS thin /dev/mapper/$POOL_NAME 1"
    dmsetup resume base-image-1

    dd if="$BASE_IMAGE" of=/dev/mapper/base-image-1 bs=4M status=progress
    sync

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
