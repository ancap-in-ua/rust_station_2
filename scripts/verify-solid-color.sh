#!/usr/bin/env bash
# Static structural checks specific to the `solid-color` tool, on top of
# the generic ones in verify-elf.sh.
#
# Unlike verify-elf.sh's checks (and unlike ps2-rt::gs's const-asserted
# bit-packing arithmetic, checked in tools/solid-color/src/main.rs
# itself), this one deliberately pins down compiled instruction bytes --
# appropriate here specifically because ps2_rt::syscall::set_gs_crt's
# body is hand-written raw MIPS assembly (`global_asm!`), not something
# rustc/LLVM chooses an encoding for. Its exact bytes are the actual
# claim being made (syscall number 2, in $v1, via a bare `syscall`
# instruction) -- see docs/progress.md 2026-11-26 and ps2-rt/src/syscall.rs
# for where that's sourced from.
set -euo pipefail

elf="${1:?usage: verify-solid-color.sh <path-to-elf>}"

if ! command -v readelf >/dev/null || ! command -v nm >/dev/null; then
    echo "verify-solid-color: readelf and nm (binutils) are required" >&2
    exit 1
fi

fail=0

check() {
    local desc="$1" got="$2" want="$3"
    if [[ "$got" == "$want" ]]; then
        echo "ok    $desc ($got)"
    else
        echo "FAIL  $desc: got $got, want $want" >&2
        fail=1
    fi
}

addr="$(nm "$elf" | awk '$3 == "set_gs_crt" {print $1}')"
if [[ -z "$addr" ]]; then
    echo "FAIL  set_gs_crt: symbol not found in $elf" >&2
    exit 1
fi

# The hand-written stub is exactly 4 instructions (16 bytes):
#   li $3, 2      -> addiu $3, $zero, 2  = 0x24030002
#   syscall                              = 0x0000000c
#   jr $31                               = 0x03e00008
#   nop                                  = 0x00000000
# readelf -x prints raw bytes in file order; for this little-endian
# target that's each instruction word's bytes LSB-first, so e.g.
# 0x24030002 appears as "02000324".
line="$(readelf -x .text "$elf" | grep -i "0x00$addr\b" || true)"
if [[ -z "$line" ]]; then
    # set_gs_crt's address from nm is a full 0x-prefixed hex value already
    # on some toolchains and a bare hex string on others -- try both.
    line="$(readelf -x .text "$elf" | grep -i "0x$(printf '%08x' "$((16#$addr))")\b" || true)"
fi
bytes="$(awk '{print $2, $3, $4, $5}' <<<"$line")"

check "set_gs_crt raw bytes (li \$3,2 / syscall / jr \$31 / nop)" \
    "$bytes" "02000324 0c000000 0800e003 00000000"

if [[ "$fail" -ne 0 ]]; then
    echo "verify-solid-color: FAILED" >&2
    exit 1
fi
echo "verify-solid-color: all checks passed"
