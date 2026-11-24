#!/usr/bin/env bash
# Boot a built ELF directly in PCSX2 (Qt frontend, nixpkgs' `pcsx2` package
# installs the binary as `pcsx2-qt`). If your PCSX2 build names/flags this
# differently, check `pcsx2-qt -help`.
set -euo pipefail

elf="${1:?usage: run-pcsx2.sh <path-to-elf>}"

exec pcsx2-qt -elf "$elf" -fastboot
