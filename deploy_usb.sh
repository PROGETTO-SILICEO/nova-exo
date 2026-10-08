#!/usr/bin/env bash
# Deploy del kernel Exo sulla chiavetta.
#
# Da lanciare con sudo (il guardiano richiede la password, quindi questo
# passo lo fa Alfonso):
#
#     sudo ~/Documenti/GitHub/nova-exo/deploy_usb.sh /dev/sdd
#
# Il kernel è quello appena compilato (con la tastiera e u,p,n).
# ATTENZIONE: la partizione viene riformattata. /dev/sdd è la chiavetta da
# 7,4 GB — NON sda/sdb (dischi interni) né sdc (Backup).
set -euo pipefail

DEV="${1:-/dev/sdd}"
PROJ="$(cd "$(dirname "$0")" && pwd)"
KERNEL="$PROJ/target/x86_64-unknown-none/release/nova-exo"
LIMINE="$PROJ/build/limine-12.3.3"
MNT=/mnt/exo-usb

if [ "$(id -u)" -ne 0 ]; then
    echo "Serve sudo: sudo $0 $DEV" >&2
    exit 1
fi

if [ ! -f "$KERNEL" ]; then
    echo "Kernel non compilato: $KERNEL" >&2
    echo "Prima: cd $PROJ && PATH=\$HOME/.cargo/bin:\$PATH make build" >&2
    exit 1
fi

echo "=== kernel da installare ==="
ls -la "$KERNEL"
echo "=== dispositivo ==="
lsblk -o NAME,SIZE,TYPE,MOUNTPOINT "$DEV"
echo
read -r -p "Confermi la scrittura su $DEV ? [s/N] " ok
[ "$ok" = "s" ] || { echo "annullato"; exit 1; }

umount "${DEV}1" 2>/dev/null || true
mkdir -p "$MNT"
mount "${DEV}1" "$MNT"

mkdir -p "$MNT/boot/limine" "$MNT/EFI/BOOT"
cp "$KERNEL" "$MNT/boot/"
cp "$PROJ/limine.conf" "$MNT/boot/limine/"
cp "$PROJ/limine.conf" "$MNT/EFI/BOOT/"
cp "$LIMINE/limine-bios.sys" "$MNT/boot/limine/"
cp "$LIMINE/BOOTX64.EFI" "$MNT/EFI/BOOT/BOOTX64.EFI"

sync
umount "$MNT"

# Limine nel MBR: senza questo la chiavetta non è avviabile in BIOS
"$LIMINE/limine" bios-install "$DEV"

echo
echo "=== FATTO. La chiavetta è pronta. ==="
echo "Metti la chiavetta nel Lenovo e avvia da USB."
echo "Il kernel ha: tastiera (il contatto) + u,p,n nella telemetria."
