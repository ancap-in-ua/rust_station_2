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

# GS privileged registers are 64-bit and are documented as not
# byte/word addressable, so every write must be a single `sd`. That is
# not something the source can express on this target:
# `write_volatile::<u64>` under o32 lowers to a *pair* of 32-bit `sw`s
# to addr and addr+4, which is exactly the access pattern the hardware
# docs rule out. It happened to be right while this was an n64/ELF64
# target and silently broke when it became o32 (2026-11-27), with no
# source change and nothing else to notice it -- hence this check.
# ps2_rt::gs::write_reg now hand-writes the `sd`; this counts them.
#
# Counted from raw bytes rather than a disassembler so this needs
# nothing readelf doesn't already provide. MIPS opcode 0x3F (SD) sits
# in bits 31-26, so a little-endian instruction word is an `sd` exactly
# when its most significant byte is 0xFC-0xFF (opcode 62 would be
# 0xF8-0xFB, so there's no overlap). readelf -x prints each word's
# bytes LSB-first, making byte 4 the MSB -- characters 7-8 of the group.
#
# solid-color makes 8 GS register writes: CSR twice, IMR, SMODE2,
# DISPFB1 and DISPLAY1 via gs::init_display, then BGCOLOR and PMODE.
# Bump this if that changes; a count of 0 means write_reg regressed to
# split stores.
sd_count="$(readelf -x .text "$elf" | awk '
    $1 ~ /^0x/ {
        for (i = 2; i <= 5; i++) {
            if (length($i) == 8) {
                msb = strtonum("0x" substr($i, 7, 2))
                if (and(msb, 0xFC) == 0xFC) n++
            }
        }
    }
    END { print n+0 }')"
check "GS register writes are single 64-bit \`sd\` stores" "$sd_count" "8"

if [[ "$fail" -ne 0 ]]; then
    echo "verify-solid-color: FAILED" >&2
    exit 1
fi
echo "verify-solid-color: all checks passed"
