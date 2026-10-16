#!/usr/bin/env bash
# tools/carica_chiavetta.sh — Script protetto per scrittura USB Nova Exo + Il Nucleo
set -euo pipefail

DEV="${1:-/dev/sdd}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "════════════════════════════════════════════════════════════════════"
echo "🕯️  NOVA EXO — Caricamento Chiavetta USB con Nucleo (~41M) & AHCI"
echo "════════════════════════════════════════════════════════════════════"
echo

# 1. Blocco di sicurezza rigido
if [[ "${DEV}" == "/dev/sda" || "${DEV}" == "/dev/sdb" || "${DEV}" == "/dev/sdc" ]]; then
    echo "❌ BLOCCO DI SICUREZZA: ${DEV} è un disco protetto di sistema o backup!"
    echo "   Operazione annullata immediatamente."
    exit 1
fi

if [[ ! -b "${DEV}" ]]; then
    echo "❌ Dispositivo non trovato: ${DEV}"
    echo "   Assicurati che la chiavetta sia inserita (controlla con lsblk)."
    exit 1
fi

echo "📌 Dispositivo target selezionato: ${DEV}"
lsblk -o NAME,SIZE,TYPE,TRAN,FSTYPE,LABEL,MOUNTPOINT,MODEL "${DEV}"
echo

read -p "Vuoi procedere con la formattazione e installazione su ${DEV}? (s/N): " -r CONFIRM
if [[ "${CONFIRM}" != "s" && "${CONFIRM}" != "S" && "${CONFIRM}" != "si" && "${CONFIRM}" != "SI" ]]; then
    echo "Operazione annullata."
    exit 0
fi

cd "${ROOT_DIR}"

# 2. Verifica o genera nucleo_41m.bin
if [[ ! -f "nucleo_41m.bin" ]]; then
    echo "── Generazione nucleo_41m.bin sintetico..."
    python3 tools/pack_nucleo_bin.py nucleo_41m.bin
fi

# 3. Compilazione release se necessaria
echo "── Verifica compilazione kernel release..."
cargo build --target x86_64-unknown-none --release
make check-elf ELF_FILE=target/x86_64-unknown-none/release/nova-exo

# 4. Partizionamento GPT + ESP su DEV
echo "── Partizionamento GPT ed ESP su ${DEV}..."
sudo parted -s "${DEV}" mklabel gpt
sudo parted -s "${DEV}" mkpart primary fat32 1MiB 100%
sudo parted -s "${DEV}" set 1 esp on
sleep 1

# 5. Formattazione FAT32 della prima partizione
PART="${DEV}1"
if [[ "${DEV}" =~ [0-9]$ ]]; then
    PART="${DEV}p1"
fi
echo "── Formattazione FAT32 su ${PART}..."
sudo mkfs.fat -F32 "${PART}"

# 6. Montaggio e copia file
MNT="/mnt/usb_nova_exo"
sudo mkdir -p "${MNT}"
echo "── Montaggio su ${MNT}..."
sudo mount "${PART}" "${MNT}"

sudo mkdir -p "${MNT}/boot/limine" "${MNT}/EFI/BOOT"
echo "── Copia kernel nova-exo..."
sudo cp target/x86_64-unknown-none/release/nova-exo "${MNT}/boot/"

echo "── Copia nucleo_41m.bin (~20 MB)..."
sudo cp nucleo_41m.bin "${MNT}/boot/"

echo "── Copia configurazione Limine e bootloader UEFI/BIOS..."
sudo cp limine.conf "${MNT}/boot/limine/"
sudo cp limine.conf "${MNT}/EFI/BOOT/"
sudo cp build/limine-12.3.3/limine-bios.sys "${MNT}/boot/limine/"
sudo cp build/limine-12.3.3/BOOTX64.EFI "${MNT}/EFI/BOOT/BOOTX64.EFI"

sync
echo "── Smontaggio..."
sudo umount "${MNT}"
sudo rmdir "${MNT}"

# 7. Installazione bootloader BIOS per compatibilità ibrida
echo "── Installazione stadio Limine BIOS su ${DEV}..."
sudo build/limine-12.3.3/limine bios-install "${DEV}"

echo
echo "════════════════════════════════════════════════════════════════════"
echo "✅ CHIAVETTA PRONTA CON SUCCESSO SU ${DEV}!"
echo "   Contiene:"
echo "   - Kernel Nova Exo (v0.24) con driver AHCI SATA LBA48 & APIC timer"
echo "   - Modulo Il Nucleo (~41M) in /boot/nucleo_41m.bin"
echo "   - Bootloader dual-mode UEFI + BIOS (Limine con menu 5s)"
echo "════════════════════════════════════════════════════════════════════"
