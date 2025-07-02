# Nuke built-in rules and variables
MAKEFLAGS += -rR
.SUFFIXES:

override OUTPUT := kernel.elf
override BUILD_DIR := ../build

override RUST_TARGET := x86_64-unknown-none

# Build profile settings
ifeq ($(RUST_PROFILE),)
    override RUST_PROFILE := dev
endif

override RUST_PROFILE_SUBDIR := $(RUST_PROFILE)
ifeq ($(RUST_PROFILE),dev)
    override RUST_PROFILE_SUBDIR := debug
endif

# Default build target
.PHONY: all
all:
	RUSTFLAGS="-C relocation-model=static" cargo build --target $(RUST_TARGET) --profile $(RUST_PROFILE)
	mkdir -p $(BUILD_DIR) && cp target/$(RUST_TARGET)/$(RUST_PROFILE_SUBDIR)/kernel $(BUILD_DIR)/$(OUTPUT)

# Test build target
.PHONY: test
test:
	RUSTFLAGS="-C relocation-model=static" cargo build --tests --target ../custom-targets/x86_64-unknown-none-no-pie.json --profile $(RUST_PROFILE)
	mkdir -p $(BUILD_DIR)
	# Find the test binary and copy it
	@TEST_BINARY=$$(find target/x86_64-unknown-none-no-pie/$(RUST_PROFILE_SUBDIR)/deps -name "kernel-*" -type f -executable | head -1); \
	if [ -n "$$TEST_BINARY" ]; then \
		echo "Copying test binary: $$TEST_BINARY"; \
		cp "$$TEST_BINARY" $(BUILD_DIR)/kernel-test.elf; \
	else \
		echo "Error: No test binary found"; \
		exit 1; \
	fi

# Clean targets
.PHONY: clean
clean:
	cargo clean
	rm -rf $(BUILD_DIR)

.PHONY: distclean
distclean: clean