.DEFAULT_GOAL := help

# ---------------------------------------------------------------------------
# Configurable variables (override on the command line, e.g. POOL_SIZE=30G)
# ---------------------------------------------------------------------------
POOL_NAME     ?= thin-pool-0
POOL_SIZE     ?= 20G
POOL_DATA_DIR ?= /var/lib/microvisor
BASE_IMAGE    ?= resources/browser-rootfs.ext4

# ---------------------------------------------------------------------------
# Derived paths — these are the outputs Make tracks for idempotency
# ---------------------------------------------------------------------------
JAILER_SRC    := resources/release-v1.10.1-x86_64/jailer-v1.10.1-x86_64
KERNEL_SRC    := resources/vmlinux-6.1.bin
KERNEL_LINK   := resources/vmlinux
POOL_SENTINEL := resources/.pool-ready

# ---------------------------------------------------------------------------
# Phonies — targets that don't produce a file of the same name
# ---------------------------------------------------------------------------
.PHONY: help all check jailer kernel pool teardown clean run

# ---------------------------------------------------------------------------
# help — default target, printed when you run `make` with no arguments
# ---------------------------------------------------------------------------
help:
	@echo ""
	@echo "  Microvisor build pipeline"
	@echo ""
	@echo "  make all          Full setup: jailer + kernel symlink + DM pool"
	@echo "  make check        Verify all prerequisites are in place"
	@echo "  make jailer       Promote bin/jailer from the release directory"
	@echo "  make kernel       Create resources/vmlinux symlink → vmlinux-6.1.bin"
	@echo "  make pool         Create DM thin pool + import base image  [needs sudo]"
	@echo "  make teardown     Tear down pool and detach loop devices   [needs sudo]"
	@echo "  make clean        teardown + remove generated symlinks and sentinels"
	@echo "  make run          Build and run the control-plane orchestrator"
	@echo ""
	@echo "  Variables: POOL_NAME=$(POOL_NAME)  POOL_SIZE=$(POOL_SIZE)"
	@echo "             BASE_IMAGE=$(BASE_IMAGE)"
	@echo ""

# ---------------------------------------------------------------------------
# all — full setup in dependency order
# ---------------------------------------------------------------------------
all: bin/jailer $(KERNEL_LINK) $(POOL_SENTINEL)
	@echo ""
	@echo "  Setup complete. Run 'make check' to verify, 'make run' to boot a VM."
	@echo ""

# ---------------------------------------------------------------------------
# check — verify everything is in place before attempting to boot
# ---------------------------------------------------------------------------
check:
	@echo "Checking prerequisites..."
	@test -x bin/firecracker  || { echo "  MISSING: bin/firecracker"; exit 1; }
	@test -x bin/jailer       || { echo "  MISSING: bin/jailer  (run: make jailer)"; exit 1; }
	@test -f $(KERNEL_LINK)   || { echo "  MISSING: $(KERNEL_LINK)  (run: make kernel)"; exit 1; }
	@test -f $(BASE_IMAGE)    || { echo "  MISSING: $(BASE_IMAGE)"; exit 1; }
	@test -f $(POOL_SENTINEL) || { echo "  MISSING: pool not set up  (run: sudo make pool)"; exit 1; }
	@test -b /dev/mapper/$(POOL_NAME) \
		|| { echo "  MISSING: /dev/mapper/$(POOL_NAME) not active  (run: sudo make pool)"; exit 1; }
	@echo "  All prerequisites OK"

# ---------------------------------------------------------------------------
# jailer — promote the jailer binary from the Firecracker release directory
# ---------------------------------------------------------------------------
jailer: bin/jailer

bin/jailer: $(JAILER_SRC)
	cp $(JAILER_SRC) bin/jailer
	chmod +x bin/jailer
	@echo "  jailer ready at bin/jailer"

# ---------------------------------------------------------------------------
# kernel — create a stable resources/vmlinux symlink the orchestrator expects
# ---------------------------------------------------------------------------
kernel: $(KERNEL_LINK)

$(KERNEL_LINK): $(KERNEL_SRC)
	ln -sf vmlinux-6.1.bin $(KERNEL_LINK)
	@echo "  kernel symlink ready: resources/vmlinux → vmlinux-6.1.bin"

# ---------------------------------------------------------------------------
# pool — create the DM thin pool and import the base rootfs image as volume #1
pool: $(POOL_SENTINEL)

#
# Requires root. The script is idempotent:
#   - If the pool is already active, exits immediately.
#   - If the backing files exist (e.g. after a reboot), re-attaches without
#     re-importing the base image.
#   - If the backing files don't exist, creates them and imports from scratch.
# ---------------------------------------------------------------------------
$(POOL_SENTINEL): $(BASE_IMAGE) bin/jailer $(KERNEL_LINK)
	sudo POOL_NAME="$(POOL_NAME)" POOL_SIZE="$(POOL_SIZE)" \
	     POOL_DATA_DIR="$(POOL_DATA_DIR)" BASE_IMAGE="$(BASE_IMAGE)" \
	     bash scripts/setup-pool.sh

# ---------------------------------------------------------------------------
# teardown — remove the DM pool and detach loop devices
#
# Backing files in /var/lib/microvisor/ are preserved so that re-running
# 'make pool' reattaches quickly without re-importing the base image.
# To start completely fresh: sudo scripts/teardown-pool.sh --purge
# ---------------------------------------------------------------------------
teardown:
	sudo POOL_NAME="$(POOL_NAME)" POOL_DATA_DIR="$(POOL_DATA_DIR)" \
	     bash scripts/teardown-pool.sh

# ---------------------------------------------------------------------------
# clean — teardown + remove generated files so 'make all' starts fresh
# ---------------------------------------------------------------------------
clean: teardown
	rm -f $(KERNEL_LINK) $(POOL_SENTINEL)
	@echo "  Clean complete. bin/jailer preserved (re-copy is instant)."

# ---------------------------------------------------------------------------
# run — build and run the control-plane (requires pool to be active)
# ---------------------------------------------------------------------------
run: check
	cargo run --manifest-path control-plane/Cargo.toml
