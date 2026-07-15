# Nova Exo v0.1 — Makefile
#
# Dipendenze: 
#   - Rust nightly + target x86_64-unknown-none (auto via rust-toolchain.toml)
#   - limine bootloader (make install-deps)
#   - qemu-system-x86_64 + OVMF (make install-deps)

KERNEL   := nova-exo
TARGET   := x86_64-unknown-none
BUILD    := target/$(TARGET)/release
ISO_DIR  := build/iso
LIMINE   := build/limine-12.3.3

# ── Build ───────────────────────────────────────────────────────────────────

.PHONY: all build clean install-deps run run-bios run-uefi usb demo video-demo build-demo iso-demo

all: build

build:
	cargo build --target $(TARGET) --release
	$(MAKE) check-elf ELF_FILE=$(BUILD)/$(KERNEL)

# Verifica allineamento pagina dei segmenti LOAD (p_offset % 0x1000 == p_vaddr % 0x1000)
# Previene il bug `-n` nmagic che ha bloccato 4 ore il 2026-07-11
ELF_FILE ?= $(BUILD)/$(KERNEL)
check-elf:
	@echo "=== Checking ELF segment alignment ==="; \
	readelf -l $(ELF_FILE) | awk '/^  LOAD/ { \
		off = strtonum("0x"$$2); \
		vaddr = strtonum("0x"$$3); \
		if ((off % 4096) != (vaddr % 4096)) { \
			print "FAIL: p_offset="$$2" p_vaddr="$$3" — mismatch"; \
			exit 1; \
		} \
	}'; \
	if [ $$? -eq 0 ]; then echo "=== Alignment OK ==="; fi

clean:
	cargo clean
	rm -rf build/

# ── Install dependencies ────────────────────────────────────────────────────

install-deps:
	@echo "=== Installing QEMU, OVMF, Limine ==="
	sudo apt-get install -y qemu-system-x86 ovmf
	@echo "=== Downloading Limine bootloader ==="
	mkdir -p build
	cd build && curl -LO https://github.com/limine-bootloader/limine/releases/download/v12.3.3/limine-12.3.3-binary.tar.gz
	cd build && tar xzf limine-12.3.3-binary.tar.gz
	@echo "=== Done ==="

# ── Bootable ISO (UEFI+BIOS) ────────────────────────────────────────────────

$(BUILD)/$(KERNEL): build

$(LIMINE):
	@echo "Run 'make install-deps' first or place limine binaries in build/limine/"
	@exit 1

iso: $(BUILD)/$(KERNEL) $(LIMINE)
	rm -rf $(ISO_DIR)
	mkdir -p $(ISO_DIR)/boot/limine
	mkdir -p $(ISO_DIR)/EFI/BOOT
	cp $(BUILD)/$(KERNEL) $(ISO_DIR)/boot/
	cp limine.conf $(ISO_DIR)/boot/limine/
	cp limine.conf $(ISO_DIR)/EFI/BOOT/
	cp $(LIMINE)/limine-bios.sys $(ISO_DIR)/boot/limine/
	cp $(LIMINE)/limine-bios-cd.bin $(ISO_DIR)/boot/limine/
	cp $(LIMINE)/BOOTX64.EFI $(ISO_DIR)/EFI/BOOT/BOOTX64.EFI
	xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot EFI/BOOT/BOOTX64.EFI \
		-efi-boot-part --efi-boot-image \
		-o build/nova-exo.iso $(ISO_DIR)
	$(LIMINE)/limine bios-install build/nova-exo.iso
	@echo "=== ISO ready: build/nova-exo.iso ==="

# ── USB bootable ────────────────────────────────────────────────────────────

usb: $(BUILD)/$(KERNEL) $(LIMINE)
	@echo "=== Preparing USB ==="
	@echo "WARNING: This will overwrite /dev/sdb! Edit the Makefile to change the device."
	@echo "Usage: DEV=/dev/sdX make usb"
	$(eval DEV ?= /dev/sdb)
	@echo "Using device: $(DEV)"
	# Create partition table
	sudo parted $(DEV) mklabel gpt
	sudo parted $(DEV) mkpart primary fat32 1MiB 100%
	sudo parted $(DEV) set 1 esp on
	# Format
	sudo mkfs.fat -F32 $(DEV)1
	# Mount and copy
	mkdir -p /mnt/usb
	sudo mount $(DEV)1 /mnt/usb
	sudo mkdir -p /mnt/usb/boot/limine /mnt/usb/EFI/BOOT
	sudo cp $(BUILD)/$(KERNEL) /mnt/usb/boot/
	sudo cp limine.conf /mnt/usb/boot/limine/
	sudo cp limine.conf /mnt/usb/EFI/BOOT/
	sudo cp $(LIMINE)/limine-bios.sys /mnt/usb/boot/limine/
	sudo cp $(LIMINE)/BOOTX64.EFI /mnt/usb/EFI/BOOT/BOOTX64.EFI
	sudo umount /mnt/usb
	$(LIMINE)/limine bios-install $(DEV)
	@echo "=== USB ready. Boot the Lenovo from USB ==="

# ── QEMU test (UEFI) ────────────────────────────────────────────────────────

run: iso
	qemu-system-x86_64 \
		-machine pc-q35-3.0,accel=kvm:hax:whpx:tcg \
		-cpu host \
		-m 512M \
		-bios /usr/share/OVMF/OVMF_CODE.fd \
		-cdrom build/nova-exo.iso \
		-serial stdio \
		-debugcon file:qemu-debug.log

# ── QEMU test (BIOS) ────────────────────────────────────────────────────────

run-bios: iso
	stdbuf -oL qemu-system-x86_64 \
		-cpu qemu64 \
		-m 512M \
		-cdrom build/nova-exo.iso \
		-serial stdio \
		-debugcon file:qemu-debug.log

# ── Demo build (with PF trigger for recordings) ───────────────────────────

BUILD_DEMO := target/$(TARGET)/release
ISO_DEMO_DIR := build/iso-demo

build-demo:
	cargo build --target $(TARGET) --release --features demo_pf
	$(MAKE) check-elf ELF_FILE=$(BUILD_DEMO)/$(KERNEL)

iso-demo: build-demo $(LIMINE)
	rm -rf $(ISO_DEMO_DIR)
	mkdir -p $(ISO_DEMO_DIR)/boot/limine
	mkdir -p $(ISO_DEMO_DIR)/EFI/BOOT
	cp $(BUILD_DEMO)/$(KERNEL) $(ISO_DEMO_DIR)/boot/
	cp limine.conf $(ISO_DEMO_DIR)/boot/limine/
	cp limine.conf $(ISO_DEMO_DIR)/EFI/BOOT/
	cp $(LIMINE)/limine-bios.sys $(ISO_DEMO_DIR)/boot/limine/
	cp $(LIMINE)/limine-bios-cd.bin $(ISO_DEMO_DIR)/boot/limine/
	cp $(LIMINE)/BOOTX64.EFI $(ISO_DEMO_DIR)/EFI/BOOT/BOOTX64.EFI
	xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot EFI/BOOT/BOOTX64.EFI \
		-efi-boot-part --efi-boot-image \
		-o build/nova-exo-demo.iso $(ISO_DEMO_DIR)
	$(LIMINE)/limine bios-install build/nova-exo-demo.iso
	@echo "=== Demo ISO ready: build/nova-exo-demo.iso ==="

# ── Live video demo — QEMU + visualizer ─────────────────────────────────

video-demo: iso-demo
	@echo "=== Nova Exo Demo — avvio QEMU + visualizer ==="
	@echo "    Premi Ctrl-C per uscire"
	stdbuf -oL qemu-system-x86_64 \
		-cpu qemu64 \
		-m 512M \
		-cdrom build/nova-exo-demo.iso \
		-serial stdio \
		-debugcon file:qemu-debug.log 2>/dev/null \
	| python3 tools/demo.py

# ── QEMU test (UEFI, no accelerator) ────────────────────────────────────────

run-uefi: iso
	qemu-system-x86_64 \
		-machine q35 \
		-cpu max \
		-m 512M \
		-bios /usr/share/OVMF/OVMF_CODE.fd \
		-cdrom build/nova-exo.iso \
		-serial stdio \
		-debugcon file:qemu-debug.log

# ── QEMU test (UEFI, raw disk image with ESP) ───────────────────────────────

HDD_IMG := build/nova-exo.img
HDD_OFFSET := 1048576

$(HDD_IMG): $(BUILD)/$(KERNEL) $(LIMINE) limine.conf linker.ld
	rm -f $(HDD_IMG)
	qemu-img create -f raw $(HDD_IMG) 64M
	sgdisk -n 1:2048:131038 -t 1:ef00 -c 1:\"EFI\" $(HDD_IMG)
	mformat -i $(HDD_IMG)@@$(HDD_OFFSET) -F -h 64 -s 32 -T 130990 -v NOVAEXO
	mmd -i $(HDD_IMG)@@$(HDD_OFFSET) ::/EFI
	mmd -i $(HDD_IMG)@@$(HDD_OFFSET) ::/EFI/BOOT
	mmd -i $(HDD_IMG)@@$(HDD_OFFSET) ::/boot
	mmd -i $(HDD_IMG)@@$(HDD_OFFSET) ::/boot/limine
	mcopy -i $(HDD_IMG)@@$(HDD_OFFSET) $(BUILD)/$(KERNEL) ::/boot/nova-exo
	mcopy -i $(HDD_IMG)@@$(HDD_OFFSET) limine.conf ::/boot/limine/limine.conf
	mcopy -i $(HDD_IMG)@@$(HDD_OFFSET) limine.conf ::/EFI/BOOT/limine.conf
	mcopy -i $(HDD_IMG)@@$(HDD_OFFSET) limine.conf ::/limine.conf
	mcopy -i $(HDD_IMG)@@$(HDD_OFFSET) $(LIMINE)/BOOTX64.EFI ::/EFI/BOOT/BOOTX64.EFI
	mcopy -i $(HDD_IMG)@@$(HDD_OFFSET) $(LIMINE)/limine-bios.sys ::/boot/limine/limine-bios.sys

run-uefi-hdd: $(HDD_IMG)
	qemu-system-x86_64 \
		-machine q35 \
		-cpu max \
		-m 512M \
		-bios /usr/share/OVMF/OVMF_CODE.fd \
		-drive file=$(HDD_IMG),format=raw,if=virtio \
		-serial stdio \
		-debugcon file:qemu-debug.log
