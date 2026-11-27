#!/usr/bin/env bash
# Dynamic checks on a built PS2 ELF: actually execute it, under
# qemu-system-mips64el, and assert what it did.
#
# This is the counterpart to verify-elf.sh's static checks. Where that
# one asserts the ELF's *shape* (entry point, _gp/_stack_top formulas,
# bss size, .got contents), this one boots the same unmodified release
# ELF and asserts its *behaviour*:
#
#   - it reaches its terminal state (`ps2_rt_halt`) at all, rather than
#     panicking, faulting, or wandering off;
#   - `$gp` and `$sp` at that point hold what link.ld says they should,
#     which is the first real test of `_start`'s hand-written assembly;
#   - `boot.rs`'s zero_bss() actually zeroed bss -- checked
#     non-vacuously, by writing a 0xd5 pattern over the whole bss range
#     *before* the first instruction runs and confirming not one of
#     those bytes survives (fresh qemu RAM reads back as zero, so
#     without pre-dirtying it this check would pass even if zero_bss
#     were a no-op);
#   - the tool's proof-of-life word landed at the expected address with
#     the expected value, which for `alloc-hello` transitively means the
#     bump allocator handed out usable, non-aliasing memory.
#
# See scripts/lib-qemu.sh for why qemu can run a PS2 ELF unmodified, and
# for the (substantial) list of things it cannot prove -- notably
# anything involving the GS or BIOS syscalls.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-qemu.sh
source "$here/lib-qemu.sh"

elf="${1:?usage: verify-qemu.sh <path-to-elf> <expected-proof-word>}"
expected_proof="${2:?usage: verify-qemu.sh <path-to-elf> <expected-proof-word>}"

qemu_require_tools

# The byte written over bss before boot. Any value works as long as it
# isn't 0 (which is what we're checking *for*) and isn't a plausible
# result a tool might legitimately leave behind.
dirty_byte_octal='\325' # 0xd5

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

halt_addr="$(elf_sym "$elf" ps2_rt_halt)"
panicked_addr="$(elf_sym "$elf" ps2_rt_panicked)"
# Looked up by symbol, not hardcoded: the proof-of-life slot is a static
# in the tool's own image (see ps2_rt::PS2_RT_PROOF). It used to be a
# fixed 0x1000, which turned out to be unwritable on a real PS2.
proof_addr="$(elf_sym "$elf" PS2_RT_PROOF)"
gp_addr="$(elf_sym "$elf" _gp)"
stack_top="$(elf_sym "$elf" _stack_top)"
heap_start="$(elf_sym "$elf" _heap_start)"
bss_start="$(elf_sym "$elf" __bss_start)"
bss_end="$(elf_sym "$elf" __bss_end)"

for required in "ps2_rt_halt:$halt_addr" "_gp:$gp_addr" "_stack_top:$stack_top" \
    "_heap_start:$heap_start" "__bss_start:$bss_start" "__bss_end:$bss_end" \
    "PS2_RT_PROOF:$proof_addr"; do
    if [[ -z "${required#*:}" ]]; then
        echo "verify-qemu: $elf has no \`${required%%:*}\` symbol -- is it a" \
            "ps2-rt ELF, and does its rust_main end in ps2_rt::ps2_rt_halt()?" >&2
        exit 1
    fi
done

bss_size=$(( bss_end - bss_start ))

work="$(mktemp -d -t verify-qemu.XXXXXX)"
cleanup() {
    qemu_cleanup
    rm -rf "$work"
}
trap cleanup EXIT

qemu_launch "$elf"

# Build the gdb driver as a file rather than a pile of -ex flags: it
# keeps the printf quoting legible, and `continue` here is the one
# genuinely open-ended step (a tool that hangs instead of halting never
# comes back), so the whole thing runs under `timeout`.
script="$work/driver.gdb"
{
    echo 'set confirm off'
    echo 'set pagination off'
    echo 'set height 0'
    # The ELF is ELF32 (o32), so gdb infers a 32-bit MIPS architecture
    # and then rejects qemu's register dump: qemu models a 64-bit CPU
    # (R4000 -- see lib-qemu.sh) and sends 64-bit registers, so gdb
    # errors with "Remote 'g' packet reply is too long". Forcing the
    # 64-bit ISA before connecting makes the register widths agree while
    # still using the ELF32 file for symbols -- which is exactly the
    # real EE situation: 64-bit CPU, 32-bit pointers.
    echo 'set architecture mips:isa64'
    echo "target remote :$QEMU_PORT"

    # The guest is frozen at the ELF entry point and has executed
    # nothing, so this is the moment to pre-dirty bss.
    echo 'printf "RESULT entry_pc 0x%x\n", $pc'
    if (( bss_size > 0 )); then
        # `head` first, then `tr`: the other way round, head closes the
        # pipe on tr and the SIGPIPE trips `set -o pipefail`.
        head -c "$bss_size" /dev/zero \
            | tr '\0' "$dirty_byte_octal" > "$work/dirty.bin"
        echo "restore $work/dirty.bin binary $bss_start"
        echo "dump binary memory $work/bss-before.bin $bss_start $bss_end"
    fi

    echo "break *$halt_addr"
    [[ -n "$panicked_addr" ]] && echo "break *$panicked_addr"
    echo 'continue'

    echo 'printf "RESULT stop_pc 0x%x\n", $pc'
    echo 'printf "RESULT gp 0x%x\n", $gp'
    echo 'printf "RESULT sp 0x%x\n", $sp'
    echo "printf \"RESULT proof 0x%x\\n\", *(unsigned int*)$proof_addr"
    (( bss_size > 0 )) && echo "dump binary memory $work/bss-after.bin $bss_start $bss_end"
    echo 'printf "RESULT done 1\n"'
} > "$script"

gdb_out="$work/gdb.out"
gdb_status=0
# 60s is many orders of magnitude more than these tools need (they halt
# in well under a second); it's a bound on how long a *hung* guest wastes
# before being reported, not a performance budget.
timeout 60 gdb -batch -x "$script" "$elf" > "$gdb_out" 2>&1 || gdb_status=$?

result() { awk -v k="$2" '$1 == "RESULT" && $2 == k {print $3; exit}' "$1"; }

if [[ "$(result "$gdb_out" done)" != "1" ]]; then
    echo "FAIL  the ELF never reached its terminal state under qemu" >&2
    if (( gdb_status == 124 )); then
        echo "      (gdb timed out -- the guest is hung, faulting, or looping" >&2
        echo "       somewhere that is neither ps2_rt_halt nor ps2_rt_panicked)" >&2
    else
        echo "      (gdb exited $gdb_status)" >&2
    fi
    echo "--- gdb output ---" >&2
    cat "$gdb_out" >&2
    exit 1
fi

check_hex "entry PC at reset (link.ld's load address)" \
    "$(result "$gdb_out" entry_pc)" "0x100000"

# Which terminal state did it land in? Both ps2_rt_halt and
# ps2_rt_panicked just spin, so without this the two are easy to
# confuse -- a panic would otherwise show up only indirectly, as a
# proof-of-life word that never got written.
stop_pc="$(result "$gdb_out" stop_pc)"
if [[ -n "$panicked_addr" ]] && (( stop_pc == panicked_addr )); then
    echo "FAIL  terminal state: the tool PANICKED (stopped in ps2_rt_panicked)" >&2
    fail=1
else
    check_hex "terminal state (reached ps2_rt_halt, i.e. ran to completion)" \
        "$stop_pc" "$halt_addr"
fi

# _start loads $gp from link.ld's `_gp` symbol; this is that assembly
# actually having run, not just the symbol having the right value.
check_hex "\$gp at halt (== link.ld's _gp)" "$(result "$gdb_out" gp)" "$gp_addr"

# $sp won't equal _stack_top (frames have been pushed), but it must
# still be inside the region link.ld set aside for the stack -- i.e.
# below the top and above everything else the image occupies. A broken
# `lui/addiu` pair in _start would land it somewhere else entirely.
sp="$(result "$gdb_out" sp)"
if (( sp <= stack_top && sp > heap_start )); then
    echo "ok    \$sp at halt (within link.ld's stack region) ($(printf '0x%x' "$sp"))"
else
    echo "FAIL  \$sp at halt: $(printf '0x%x' "$sp") is outside" \
        "($(printf '0x%x' "$heap_start"), $(printf '0x%x' "$stack_top")]" >&2
    fail=1
fi

check_hex "proof-of-life word (PS2_RT_PROOF @ $proof_addr)" \
    "$(result "$gdb_out" proof)" "$expected_proof"

# bss zeroing, checked against the pattern written before boot. Counting
# surviving 0xd5 bytes (rather than counting non-zero bytes) is what
# makes this work for `alloc-hello`, whose bss legitimately holds
# allocator data by the time it halts.
if (( bss_size > 0 )); then
    dirty_count() { tr -dc "$dirty_byte_octal" < "$1" | wc -c; }
    # Guards against the check being vacuous: if `restore` silently
    # didn't take, "no 0xd5 bytes left afterwards" would be trivially
    # true and zero_bss could be a no-op without anyone noticing.
    check "bss was dirtied before boot (all $bss_size bytes)" \
        "$(dirty_count "$work/bss-before.bin")" "$bss_size"
    check "bss zeroed by boot.rs zero_bss (0xd5 bytes surviving)" \
        "$(dirty_count "$work/bss-after.bin")" "0"
else
    echo "ok    bss zeroing (skipped: this ELF's bss is empty)"
fi

if [[ "$fail" -ne 0 ]]; then
    echo "verify-qemu: FAILED" >&2
    exit 1
fi
echo "verify-qemu: all checks passed"
