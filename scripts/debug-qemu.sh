#!/usr/bin/env bash
# Boot a built ELF under qemu and drop into an interactive gdb attached to
# it, frozen at the entry point. For poking at a tool by hand -- the
# scripted equivalents are verify-qemu.sh / verify-solid-color-qemu.sh.
#
# qemu is not a PS2: no GS, no BIOS syscalls, no R5900-specific
# instructions. See scripts/lib-qemu.sh for what that does and doesn't
# let you conclude, and use PCSX2 (scripts/run-pcsx2.sh) for anything
# touching actual PS2 hardware.
#
# Useful once attached:
#   break *ps2_rt_halt      -- where a tool ends up when it completes
#   break *ps2_rt_panicked  -- ... and where it ends up when it panics
#   continue
#   x/1xw 0x1000            -- the tools' proof-of-life address
#   info registers pc sp gp
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-qemu.sh
source "$here/lib-qemu.sh"

elf="${1:?usage: debug-qemu.sh <path-to-elf>}"

qemu_require_tools
trap qemu_cleanup EXIT
qemu_launch "$elf"

echo "qemu up (pid $QEMU_PID), guest frozen at the entry point."
echo "attaching gdb to :$QEMU_PORT -- quitting gdb shuts qemu down too."
echo

# Not `exec`: that would replace this shell and skip the EXIT trap,
# leaving an orphaned qemu behind every time you quit gdb.
gdb \
    -ex "target remote :$QEMU_PORT" \
    -ex 'info registers pc sp gp' \
    "$elf"
