# Nuke built-in rules and variables
MAKEFLAGS += -rR
.SUFFIXES:

override QEMUFLAGS := -m 2G -serial stdio -no-reboot -no-shutdown -enable-kvm -smp 2 -cpu host,+x2apic -machine q35,accel=kvm

override IMAGE_NAME := locos

override BUILD_DIR := build
override INPUT := kernel.elf

.PHONY: all
all: $(IMAGE_NAME).iso

.PHONY: run
run: ovmf/ovmf-code-x86_64.fd ovmf/ovmf-vars-x86_64.fd $(IMAGE_NAME).iso
	qemu-system-x86_64 \
		-M q35 \
		-drive if=pflash,unit=0,format=raw,file=ovmf/ovmf-code-x86_64.fd,readonly=on \
		-drive if=pflash,unit=1,format=raw,file=ovmf/ovmf-vars-x86_64.fd \
		-cdrom $(IMAGE_NAME).iso \
		$(QEMUFLAGS)

.PHONY: test
test: ovmf/ovmf-code-x86_64.fd ovmf/ovmf-vars-x86_64.fd $(IMAGE_NAME)-test.iso
	qemu-system-x86_64 \
		-M q35 \
		-drive if=pflash,unit=0,format=raw,file=ovmf/ovmf-code-x86_64.fd,readonly=on \
		-drive if=pflash,unit=1,format=raw,file=ovmf/ovmf-vars-x86_64.fd \
		-cdrom $(IMAGE_NAME)-test.iso \
		-device isa-debug-exit,iobase=0xf4,iosize=0x04 \
		-serial stdio \
		-display none \
		-m 2G \
		-no-reboot \
		-no-shutdown \
		-enable-kvm \
		-smp 2 \
		-cpu host,+x2apic;


ovmf/ovmf-code-x86_64.fd:
	mkdir -p ovmf
	curl -Lo $@ https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/ovmf-code-x86_64.fd

ovmf/ovmf-vars-x86_64.fd:
	mkdir -p ovmf
	curl -Lo $@ https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/ovmf-vars-x86_64.fd

limine/limine:
	rm -rf limine
	git clone https://github.com/limine-bootloader/limine.git --branch=v9.x-binary --depth=1
	$(MAKE) -C limine

.PHONY: kernel
kernel:
	$(MAKE) -C kernel

.PHONY: kernel-test
kernel-test:
	$(MAKE) -C kernel test

$(IMAGE_NAME).iso: limine/limine kernel
	rm -rf iso_root
	mkdir -p iso_root/boot
	cp -v $(BUILD_DIR)/$(INPUT) iso_root/boot/
	mkdir -p iso_root/boot/limine
	cp -v limine.conf iso_root/boot/limine/
	mkdir -p iso_root/EFI/BOOT
	cp -v limine/limine-bios.sys limine/limine-bios-cd.bin limine/limine-uefi-cd.bin iso_root/boot/limine/
	cp -v limine/BOOTX64.EFI iso_root/EFI/BOOT/
	cp -v limine/BOOTIA32.EFI iso_root/EFI/BOOT/
	xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		iso_root -o $(IMAGE_NAME).iso
	./limine/limine bios-install $(IMAGE_NAME).iso
	rm -rf iso_root

$(IMAGE_NAME)-test.iso: limine/limine kernel-test
	rm -rf iso_root_test
	mkdir -p iso_root_test/boot
	cp -v $(BUILD_DIR)/kernel-test.elf iso_root_test/boot/kernel.elf
	mkdir -p iso_root_test/boot/limine
	cp -v limine.conf iso_root_test/boot/limine/
	mkdir -p iso_root_test/EFI/BOOT
	cp -v limine/limine-bios.sys limine/limine-bios-cd.bin limine/limine-uefi-cd.bin iso_root_test/boot/limine/
	cp -v limine/BOOTX64.EFI iso_root_test/EFI/BOOT/
	cp -v limine/BOOTIA32.EFI iso_root_test/EFI/BOOT/
	xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		iso_root_test -o $(IMAGE_NAME)-test.iso
	./limine/limine bios-install $(IMAGE_NAME)-test.iso
	rm -rf iso_root_test

.PHONY: clean
clean:
	$(MAKE) -C kernel clean
	rm -rf iso_root iso_root_test $(IMAGE_NAME).iso $(IMAGE_NAME)-test.iso

.PHONY: distclean
distclean: clean
	$(MAKE) -C kernel distclean
	rm -rf limine ovmf
