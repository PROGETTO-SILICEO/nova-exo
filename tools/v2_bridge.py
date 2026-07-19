#!/usr/bin/env python3
"""
v2_bridge.py — Ponte Exo ↔ Nova v2.

Lancia QEMU con Exo, legge output seriale in tempo reale,
mantiene stato strutturato, scrive exo_state.json,
forwarda comandi a Exo.

Usage:
  python3 tools/v2_bridge.py            # standalone
  make run-bridge                       # via Makefile

Nova v2 legge state/exo_state.json e scrive comandi su stdin del bridge.
"""

import json, os, re, subprocess, sys, time, signal

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STATE_FILE = os.path.join(ROOT, "state", "exo_state.json")
HDD_IMG = os.path.join(ROOT, "build", "nova-exo.img")
QEMU = "qemu-system-x86_64"

STATE = {
    "exo_version": "unknown",
    "bridge_active": True,
    "last_tick": 0,
    "patterns": 0,
    "cells": {"T": None, "C": None, "M": None, "I": None},
    "familiarity": 0.0,
    "familiarity_tick": 0,
    "attractor_sim": 0.0,
    "attractor_tick": 0,
    "beta": 0.0,
    "beta_mean": 0.0,
    "beta_conv": 0,
    "sleep_count": 0,
    "last_sleep_delta": 0.0,
    "last_sense": None,
    "last_weight_set": None,
    "sedimentation": True,
    "last_updated": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
}


def write_state():
    STATE["last_updated"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    os.makedirs(os.path.dirname(STATE_FILE), exist_ok=True)
    with open(STATE_FILE, "w") as f:
        json.dump(STATE, f, indent=2)


def parse_line(line: str):
    # T/C/M/I: state lines
    m = re.match(r"^([TCIM]):([0-9a-fA-F]+):(.+)$", line)
    if m:
        cell = m.group(1)
        STATE["cells"][cell] = m.group(3)
        return
    # F: familiarity
    m = re.match(r"^F:([\d.\-]+)@(\d+)$", line)
    if m:
        STATE["familiarity"] = float(m.group(1))
        STATE["familiarity_tick"] = int(m.group(2))
        return
    # A: attractor
    m = re.match(r"^A:([\d.\-]+)@(\d+)$", line)
    if m:
        STATE["attractor_sim"] = float(m.group(1))
        STATE["attractor_tick"] = int(m.group(2))
        return
    # β: convergence
    m = re.match(r"^β:([\d.\-]+) μ:([\d.\-]+) cv:(\d+)$", line)
    if m:
        STATE["beta"] = float(m.group(1))
        STATE["beta_mean"] = float(m.group(2))
        STATE["beta_conv"] = int(m.group(3))
        return
    # SLEEP report
    m = re.match(r"^SLEEP:(AUTO|CMD)@(\d+) → processed=(\d+) novel=(\d+) familiar=(\d+) delta=([\d.\-]+)", line)
    if m:
        STATE["sleep_count"] += 1
        STATE["last_sleep_delta"] = float(m.group(6))
        return
    # W: weight set
    m = re.match(r"^W:(IN|F) (\d+),(\d+)=([\d.\-]+)$", line)
    if m:
        STATE["last_weight_set"] = {
            "matrix": m.group(1), "i": int(m.group(2)),
            "j": int(m.group(3)), "value": float(m.group(4)),
        }
        return
    # SENS: inject
    m = re.match(r"^SENS:INJECT@([0-9a-fA-F]+)$", line)
    if m:
        STATE["last_sense"] = {"addr": int(m.group(1), 16), "tick": STATE["last_tick"]}
        return
    # P:N= pattern count
    m = re.match(r"^P:N=(\d+)$", line)
    if m:
        STATE["patterns"] = int(m.group(1))
        return
    # Version
    m = re.search(r"Nova Exo v([\d.]+)", line)
    if m:
        STATE["exo_version"] = m.group(0)


def run_bridge():
    write_state()
    os.makedirs(os.path.dirname(HDD_IMG), exist_ok=True)
    if not os.path.exists(HDD_IMG):
        print("ERROR: HDD image not found. Run 'make' first.")
        sys.exit(1)
    cmd = (
        f"{QEMU} -machine q35 -cpu max -m 512M "
        f"-bios /usr/share/OVMF/OVMF_CODE.fd "
        f"-drive file={HDD_IMG},format=raw,if=virtio "
        f"-serial stdio "
        f"-debugcon file:/dev/null "
        f"-netdev user,id=net0 "
        f"-device e1000,netdev=net0,mac=52:54:00:12:34:56 "
        f"-object filter-dump,id=dump0,netdev=net0,file=qemu-net.pcap "
        f"2>/dev/null"
    )
    p = subprocess.Popen(
        cmd, shell=True,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL, start_new_session=True,
    )
    last_write = 0.0
    try:
        while True:
            line_bytes = p.stdout.readline()
            if not line_bytes:
                break
            line = line_bytes.decode("utf-8", errors="replace").rstrip("\n\r")
            if not line:
                continue
            # Extract tick from I: line for last_tick tracking
            m = re.match(r"^I:(\d+):", line)
            if m:
                STATE["last_tick"] = int(m.group(1))
            parse_line(line)
            t = time.time()
            if t - last_write >= 0.5:
                write_state()
                last_write = t
    except (BrokenPipeError, KeyboardInterrupt):
        pass
    finally:
        STATE["bridge_active"] = False
        write_state()
        try:
            os.killpg(os.getpgid(p.pid), signal.SIGTERM)
            p.wait(timeout=5)
        except Exception:
            pass


if __name__ == "__main__":
    run_bridge()