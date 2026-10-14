#!/usr/bin/env bash
# ── inventario.sh — la MAPPA DELLE PERIFERICHE, tutto in un file ─────────────
#
# Perché esiste (14/10/2026): finora Exo ha SCOPERTO le periferiche una per una,
# e ogni volta l'hardware vero l'ha smentita (metabol, temperatura, udito sul
# codec). Un sistema maturo — Linux — le mappa GIÀ e le riporta. Invece di
# indovinare, si CHIEDE alla macchina. (Idea di Alfonso: "come fa Linux a
# riconoscere il disco", estesa a tutte le periferiche.)
#
# Da eseguire su LINUX sulla macchina da mappare (es. il Lenovo), NON su Exo.
# Alcune voci vogliono root (dmidecode, smartctl, acpidump, lshw): usa sudo.
#
#   sudo bash inventario.sh [file-output]
#
# Produce UN file con: CPU/DMI, PCI (+driver), USB, HDA codec (I NODI!),
# audio, temperatura/ventole, dischi, rete, ACPI (EC), e lshw -short.
set -u

# Salva ACCANTO allo script (se sta su una chiavetta, il risultato finisce sulla
# chiavetta: la riporti e la leggiamo). Override col primo argomento.
DIR="$(cd "$(dirname "$0")" 2>/dev/null && pwd || echo .)"
OUT="${1:-$DIR/inventario-$(hostname 2>/dev/null || echo macchina).txt}"

have() { command -v "$1" >/dev/null 2>&1; }
sez() { echo; echo "==================================================================="; echo "== $*"; echo "==================================================================="; }

{
  echo "INVENTARIO PERIFERICHE — $(date)"
  echo "host=$(hostname 2>/dev/null)  kernel=$(uname -r 2>/dev/null)"

  sez "CPU e identificazione"
  have lscpu && lscpu || head -30 /proc/cpuinfo
  grep -m1 "model name" /proc/cpuinfo 2>/dev/null

  sez "SCHEDA MADRE / BIOS (dmidecode — serve root)"
  have dmidecode && dmidecode -t system -t bios -t baseboard -t chassis 2>/dev/null | grep -Ev "^\s*$"

  sez "PCI — device, driver, moduli (nnk)"
  have lspci && lspci -nnk 2>/dev/null

  sez "USB"
  have lsusb && lsusb 2>/dev/null

  sez "!!! HDA CODEC — L'ALBERO DEI WIDGET (i NODI per l'udito) !!!"
  for f in /proc/asound/card*/codec#*; do
    [ -f "$f" ] && { echo "--- $f ---"; cat "$f"; }
  done
  echo "--- schede audio ---"
  cat /proc/asound/cards 2>/dev/null
  have aplay && aplay -l 2>/dev/null

  sez "TEMPERATURA E VENTOLE (chi le legge davvero)"
  have sensors && sensors 2>/dev/null
  echo "--- /sys/class/thermal ---"
  for z in /sys/class/thermal/thermal_zone*; do
    [ -d "$z" ] && echo "$(basename "$z") type=$(cat "$z/type" 2>/dev/null) temp=$(cat "$z/temp" 2>/dev/null)"
  done
  echo "--- /sys/class/hwmon ---"
  for h in /sys/class/hwmon/hwmon*; do
    [ -d "$h" ] && echo "$(basename "$h") name=$(cat "$h/name" 2>/dev/null)"
  done

  sez "DISCHI (chi comanda: AHCI? NVMe?)"
  lsblk -o NAME,SIZE,TYPE,FSTYPE,MODEL,SERIAL,MOUNTPOINT 2>/dev/null
  lspci 2>/dev/null | grep -i -E "sata|ahci|nvme|raid|ide"
  have smartctl && smartctl --scan 2>/dev/null

  sez "RETE"
  ip -br link 2>/dev/null
  ip -br addr 2>/dev/null

  sez "ACPI (EC / thermal — la ventola)"
  have acpidump && { acpidump 2>/dev/null | grep -A3 -i -E "ECDT|PNP0C09|thermal" | head -40; }
  ls /sys/bus/acpi/devices/ 2>/dev/null | grep -i -E "PNP0C09|therm|lid|ac" 

  sez "lshw -short (quadro d'insieme)"
  have lshw && lshw -short 2>/dev/null

  echo
  echo "=== FINE INVENTARIO ==="
} 2>&1 | tee "$OUT"

echo
echo "Salvato in: $OUT"
echo "Riportalo qui e lo leggiamo insieme."
