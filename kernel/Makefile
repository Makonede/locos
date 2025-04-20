# Nuke built-in rules and variables
MAKEFLAGS += -rR
.SUFFIXES:

override OUTPUT := kernel

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
	cp target/$(RUST_TARGET)/$(RUST_PROFILE_SUBDIR)/kernel ../kernel.elf

# Clean targets
.PHONY: clean
clean:
	cargo clean
	rm -f ../kernel.elf

.PHONY: distclean
distclean: clean