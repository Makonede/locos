# Makefile for locOS

.PHONY: all kernel bootimage run-bios run-uefi clean

all: kernel bootimage

kernel:
	cd kernel && cargo build --target x86_64-unknown-none

bootimage: kernel
	cd bootimage && cargo run

run-bios: all
	cd bootimage && cargo run --bin qemu-bios

run-uefi: all
	cd bootimage && cargo run --bin qemu-uefi

clean:
	cd kernel && cargo clean && cd ../bootimage && cargo clean
