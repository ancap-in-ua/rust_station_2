#!/usr/bin/env bash
# Static structural checks on a built PS2 ELF.
#
# !! The "ELF class == ELF64" check below asserts a KNOWN BUG. PCSX2's
# !! loader is ELF32-only and segfaults on our ELF64 binaries; PS2 ELFs
# !! are ELF32 by universal convention. This check was written during the
# !! initial scaffold, before anything had ever been booted, and it has
# !! been locking in the defect ever since. It is left as-is only so the
# !! fix (re-targeting to ELF32 -- docs/next-steps.md #1) flips it
# !! deliberately rather than silently. See docs/progress.md 2026-11-27.
#
# None of this proves the ELF actually boots under PCSX2/real hardware —
# that still needs a BIOS dump and a human in front of PCSX2 (see
# docs/next-steps.md #1). What this *does* catch is regressions in
# link.ld/the target spec breaking the invariants that were manually
# verified with readelf/nm during the initial scaffold (see
# docs/progress.md) — entry point, the _gp/_stack_top formulas, and
# relative section layout.
set -euo pipefail

elf="${1:?usage: verify-elf.sh <path-to-elf> [expected-bss-size]}"
# Most tools so far have no statics, so bss is empty by default; tools
# that do (e.g. `alloc-hello`, backed by ps2_rt::heap's static HEAP
# array) pass their expected __bss_end - __bss_start here instead.
expected_bss_size="${2:-0}"

if ! command -v readelf >/dev/null || ! command -v nm >/dev/null; then
    echo "verify-elf: readelf and nm (binutils) are required" >&2
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

# Same as `check`, but for 0x-prefixed hex values that may differ in
# leading-zero padding between readelf/nm output.
check_hex() {
    local desc="$1" got="$2" want="$3"
    check "$desc" "$(printf '0x%x' "$((got))")" "$(printf '0x%x' "$((want))")"
}

header="$(readelf -h "$elf")"
class="$(grep -m1 'Class:' <<<"$header" | awk '{print $2}')"
machine="$(grep -m1 'Machine:' <<<"$header" | sed 's/.*Machine:\s*//')"
flags="$(grep -m1 'Flags:' <<<"$header" | sed 's/.*Flags:\s*//')"
entry="$(grep -m1 'Entry point address:' <<<"$header" | awk '{print $NF}')"

check "ELF class" "$class" "ELF64"
check "machine"   "$machine" "MIPS R3000"
case "$flags" in
    *noreorder*mips3* | *mips3*noreorder*) echo "ok    flags include noreorder+mips3 ($flags)" ;;
    *) echo "FAIL  flags missing noreorder/mips3: $flags" >&2; fail=1 ;;
esac
check_hex "entry point" "$entry" "0x100000"

# readelf right-pads the `[N]` index column to a fixed width, so for
# single-digit indices it prints "[ 1]" with an internal space -- which
# throws off naive whitespace field-splitting. Collapse it to "[N]" (one
# token, no internal space) so column numbers below are consistent
# regardless of how many sections there are.
sections="$(readelf -S -W "$elf" | sed -E 's/\[[[:space:]]*[0-9]+\]/[N]/')"

# link.ld places `_gp = . + 0x8000;` right after .text/.rodata/.data, so
# "." at that point is the end of whichever of those is laid out last.
# Only .text is non-empty for `hello` today (no consts or statics yet) --
# if a future tool adds .rodata/.data content, re-check this against a
# fresh `readelf -S -W` by hand, since orphan sections ld inserts between
# them (e.g. `.MIPS.options`, seen below) aren't accounted for here.
prev_end=0
for name in .text .rodata .data; do
    read -r addr size <<<"$(awk -v n="$name" '$2 == n {print $4, $6}' <<<"$sections")"
    if [[ -n "${addr:-}" ]]; then
        end="$((0x$addr + 0x$size))"
        (( end > prev_end )) && prev_end="$end"
    fi
done

syms="$(nm "$elf")"
gp="0x$(awk '$3 == "_gp" {print $1}' <<<"$syms")"
stack_top="0x$(awk '$3 == "_stack_top" {print $1}' <<<"$syms")"
bss_start="0x$(awk '$3 == "__bss_start" {print $1}' <<<"$syms")"
bss_end="0x$(awk '$3 == "__bss_end" {print $1}' <<<"$syms")"

want_gp="$(printf '0x%x' "$((prev_end + 0x8000))")"
check_hex "_gp (== end of .text/.rodata/.data + 0x8000)" "$gp" "$want_gp"

# link.ld: 32 MiB RAM, 1 MiB reserved at the bottom for BIOS/kernel.
check_hex "_stack_top" "$stack_top" "0x2000000"

check_hex "bss size (__bss_end - __bss_start)" "$((bss_end - bss_start))" "$expected_bss_size"

# No dynamic linking happens in this static ELF, and no code anywhere in
# `hello` does a $gp-relative load/store (confirmed by hand with
# `rust-objdump -d`, see docs/progress.md 2026-11-26) -- so the only
# things that should turn up are (a) zero relocations and (b) a `.got`
# containing exactly the MIPS psABI's two mandatory reserved header
# entries (GOT[0] = 0, GOT[1] = 0x8000000000000000), not anything
# generated code actually consumes.
check "relocations" "$(readelf -r "$elf" | grep -v '^$')" "There are no relocations in this file."

got_size="$(awk '$2 == ".got" {print "0x"$6}' <<<"$sections")"
check_hex ".got size (psABI's 2 reserved header entries only)" "$got_size" "0x10"

got_bytes="$(readelf -x .got "$elf" | awk '$1 ~ /^0x/ {print $2, $3, $4, $5}')"
check ".got content (reserved header: 0, then 0x8000000000000000)" \
    "$got_bytes" "00000000 00000000 00000000 00000080"

if [[ "$fail" -ne 0 ]]; then
    echo "verify-elf: FAILED" >&2
    exit 1
fi
echo "verify-elf: all checks passed"
