#!/usr/bin/env bash
# Dynamic checks on a built PS2 ELF, run on an actual PS2: PCSX2 with a
# real BIOS.
#
# This is the only layer in the project that proves anything about a
# PlayStation 2. verify-elf.sh checks the file's shape, verify-qemu.sh
# proves the code executes on a generic MIPS CPU -- but both were fully
# green on 2026-11-27 while the ELFs were unloadable by every real PS2
# loader and wrote their results to an address that faults on hardware.
# Those bugs were only visible here (docs/progress.md).
#
# What it asserts: PCSX2's loader accepts the ELF and starts executing
# it at the expected entry point, the BIOS kernel raises no TLB miss
# (i.e. we only touched memory we're entitled to), and the tool's
# proof-of-life value reached PS2_RT_PROOF -- read over PINE rather than
# by eye.
#
# Needs a BIOS dump in bios_dump/; skips loudly without one.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-pcsx2.sh
source "$here/lib-pcsx2.sh"

elf="${1:?usage: verify-pcsx2.sh <path-to-elf> <expected-proof-word>}"
expected_proof="${2:?usage: verify-pcsx2.sh <path-to-elf> <expected-proof-word>}"

pcsx2_require_tools
pcsx2_skip_if_no_bios || exit 0

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

check_hex() {
    local desc="$1" got="$2" want="$3"
    check "$desc" "$(printf '0x%x' "$((got))")" "$(printf '0x%x' "$((want))")"
}

proof_addr="$(pcsx2_elf_sym "$elf" PS2_RT_PROOF)"
if [[ -z "$proof_addr" ]]; then
    echo "verify-pcsx2: $elf has no PS2_RT_PROOF symbol -- does its" \
        "rust_main call ps2_rt::set_proof()?" >&2
    exit 1
fi

bios="$(pcsx2_find_bios)"
trap pcsx2_cleanup EXIT
pcsx2_write_config "$bios"
pcsx2_launch "$elf"
pcsx2_wait_for_exec || exit 1

echo "ok    PCSX2 loaded and started the ELF (BIOS: $bios)"

# Entry point, straight from PCSX2's own loader rather than from readelf.
logged_entry="$(grep -m1 -o 'EntryPoint = 0x[0-9A-Fa-f]*' "$PCSX2_LOG" | head -1 | awk '{print $3}')"
check_hex "entry point PCSX2 jumped to" "${logged_entry:-0}" "0x100000"

proof="$(python3 "$here/pine.py" --wait 30 "$proof_addr" | awk '{print $2}')"
check_hex "proof-of-life word (PS2_RT_PROOF @ $proof_addr)" "$proof" "$expected_proof"

# A TLB miss means the tool touched memory the EE kernel never mapped
# for it -- which is exactly what the old fixed-address 0x1000
# proof-of-life convention did on every run.
if grep -q 'TLB Miss' "$PCSX2_LOG"; then
    echo "FAIL  the BIOS kernel reported a TLB miss:" >&2
    grep 'TLB Miss' "$PCSX2_LOG" | head -5 >&2
    echo "      (the tool touched memory it has no mapping for -- the first" >&2
    echo "       MiB belongs to the BIOS kernel; see docs/progress.md)" >&2
    fail=1
else
    echo "ok    no TLB miss (only touched memory the loader mapped for us)"
fi

if [[ "$fail" -ne 0 ]]; then
    echo "verify-pcsx2: FAILED" >&2
    exit 1
fi
echo "verify-pcsx2: all checks passed"
