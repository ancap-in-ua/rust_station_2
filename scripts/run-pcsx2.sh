#!/usr/bin/env bash
# Boot a built ELF in PCSX2 with a window, for looking at it.
#
# The scripted equivalent is verify-pcsx2.sh; this is the "show me"
# version. Uses the same bios_dump/ + .pcsx2/ setup, so it needs a BIOS
# dump too.
#
# This script used to be `pcsx2-qt -elf "$elf" -fastboot`, which boots
# nothing at all: `-elf` only *overrides* the boot ELF, the file has to
# be a positional argument, and PCSX2 ignores files that don't end in
# `.elf`. It had been wrong since the initial scaffold and nobody could
# tell, because there was no BIOS to run it with.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-pcsx2.sh
source "$here/lib-pcsx2.sh"

elf="${1:?usage: run-pcsx2.sh <path-to-elf>}"

pcsx2_require_tools
pcsx2_skip_if_no_bios || exit 1

bios="$(pcsx2_find_bios)"
pcsx2_write_config "$bios"

# Copy to a .elf name next to a temp dir we clean up on exit.
work="$(mktemp -d -t run-pcsx2.XXXXXX)"
trap 'rm -rf "$work"' EXIT
target="$work/${elf##*/}.elf"
cp -f "$elf" "$target"

echo "booting ${elf##*/} in PCSX2 (BIOS: $bios)"
"$PCSX2_BIN" -datapath "$PCSX2_DATA" -fastboot "$target"
