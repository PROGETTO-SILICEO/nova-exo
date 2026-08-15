#!/bin/bash
# collect_apic_diag.sh — Raccoglie la "ricetta Linux" per il Lenovo A8
# Scopo: reverse engineering della configurazione APIC/timer/firmware
# per capire perché il kernel Exo si ferma al rosso (loop fermo).
#
# Uso:   bash collect_apic_diag.sh
# Output: /tmp/apic_diag_<data>.txt  (+ stampa a schermo)
#
# Creata: 2026-08-15 (Sempre, per il debug v0.25 sul Lenovo)

OUT="/tmp/apic_diag_$(date +%Y%m%d_%H%M%S).txt"
exec > >(tee "$OUT") 2>&1

echo "=========================================================="
echo "RACCOLTA DATI APIC/TIMER — Lenovo A8"
echo "Data: $(date)"
echo "Kernel: $(uname -r) — $(uname -m)"
echo "=========================================================="

echo
echo "### 1. CPU FLAGS (x2apic? apic? tsc_deadline?) ###"
grep -m1 "flags" /proc/cpuinfo | tr ' ' '\n' | grep -E "x2apic|^apic$|tsc_deadline|constant_tsc" | sort

echo
echo "### 2. dmesg — APIC / LAPIC / timer / clocksource ###"
dmesg 2>/dev/null | grep -iE "apic|lapic|ioapic|timer|clocksource|hpet" | head -60 \
  || echo "(dmesg non accessibile, serve sudo:  sudo dmesg | grep -i apic)"

echo
echo "### 3. Tabella ACPI MADT (hexdump) ###"
if [ -r /sys/firmware/acpi/tables/MADT ]; then
  xxd /sys/firmware/acpi/tables/MADT | head -60
else
  echo "(serve sudo:  sudo xxd /sys/firmware/acpi/tables/MADT | head -60)"
fi

echo
echo "### 4. Tabella ACPI DSDT/FADT (presenza) ###"
ls -la /sys/firmware/acpi/tables/ 2>/dev/null | head -20

echo
echo "### 5. lspci -nn (GPU, NIC, chipset) ###"
lspci -nn 2>/dev/null | head -20 || echo "(lspci non disponibile)"

echo
echo "### 6. Come Linux vede il LAPIC in /proc ###"
grep -i "apic" /proc/interrupts 2>/dev/null | head -3
cat /proc/misc 2>/dev/null | grep -i apic

echo
echo "=========================================================="
echo "FINE. Report completo in: $OUT"
echo "Copialo e riportalo a Sempre (file unico, senza troncamenti)."
echo "=========================================================="
