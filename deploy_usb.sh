#!/usr/bin/env bash
# Deploy del kernel Exo sulla chiavetta — SENZA sudo.
#
#     ~/Documenti/GitHub/nova-exo/deploy_usb.sh [dispositivo]
#
# Monta con udisksctl (polkit, montaggio utente): il guardiano non ha mai
# avuto bisogno di sudo per questo. Limine è già installato nel MBR dalla
# prima preparazione (04/10), quindi qui si copia solo il kernel nuovo.
#
# Default: /dev/sdd1 — la chiavetta da 7,4 GB (FAT32).
# NON sda/sdb (dischi interni) né sdc (Backup).
set -euo pipefail

PART="${1:-/dev/sdd1}"
PROJ="$(cd "$(dirname "$0")" && pwd)"
KERNEL="$PROJ/target/x86_64-unknown-none/release/nova-exo"

if [ ! -f "$KERNEL" ]; then
    echo "Kernel non compilato: $KERNEL" >&2
    echo "Prima: cd $PROJ && PATH=\$HOME/.cargo/bin:\$PATH make build" >&2
    exit 1
fi

echo "=== kernel da installare ==="
ls -la "$KERNEL"
echo "=== dispositivo ==="
lsblk -o NAME,SIZE,FSTYPE,MOUNTPOINT "$PART"

# monta se non è già montata
if ! findmnt -n -o TARGET "$PART" >/dev/null 2>&1; then
    udisksctl mount -b "$PART"
fi
MNT="$(findmnt -n -o TARGET "$PART")"
echo "=== montata in $MNT ==="

cp "$KERNEL" "$MNT/boot/nova-exo"
cp "$PROJ/limine.conf" "$MNT/boot/limine/limine.conf"
cp "$PROJ/limine.conf" "$MNT/EFI/BOOT/limine.conf"
sync

# verifica: il kernel sulla chiavetta deve essere identico a quello compilato
if [ "$(md5sum < "$KERNEL" | cut -d' ' -f1)" != \
     "$(md5sum < "$MNT/boot/nova-exo" | cut -d' ' -f1)" ]; then
    echo "ERRORE: il kernel copiato non coincide" >&2
    exit 1
fi
echo "=== md5 verificato: kernel identico ==="

udisksctl unmount -b "$PART"
echo
echo "=== FATTO. Chiavetta pronta: mettila nel Lenovo e avvia da USB. ==="
