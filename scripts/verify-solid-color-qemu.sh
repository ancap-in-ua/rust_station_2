#!/usr/bin/env bash
# Dynamic checks specific to the `solid-color` tool -- the subset of its
# behaviour that an emulator with no PS2 hardware can actually settle.
#
# `solid-color` cannot be run to completion here. Its first act is a BIOS
# syscall (`ps2_rt::syscall::set_gs_crt`), and qemu has no PS2 BIOS: the
# `syscall` instruction traps to an exception vector holding no handler,
# and the guest never comes back (confirmed by hand -- stepping over that
# one instruction hangs). So it never reaches `ps2_rt_halt` and
# verify-qemu.sh cannot be used on it.
#
# What *is* checkable is everything up to that instruction, and that
# turns out to cover the claim this tool's static checks couldn't reach.
# scripts/verify-solid-color.sh pins down set_gs_crt's compiled bytes,
# and tools/solid-color/src/main.rs const-asserts the register values --
# but neither can show that execution actually *arrives* there, with
# those arguments, in the right registers. The MIPS o32/n32 calling
# convention putting the three arguments in $a0-$a2 is the entire reason
# `set_gs_crt`'s body can be four instructions with no argument
# marshalling at all (see docs/progress.md 2026-11-26), and that
# assumption is compiler-dependent, not something the source states.
# Here it gets tested: break on the symbol, read the registers.
#
# Expected arguments, from ps2sdk's graph_set_mode (ee/graph/src/
# graph_mode.c) and ps2tek's SetGsCrt(interlaced, display_mode, frame):
#   $a0 = GRAPH_MODE_NONINTERLACED = 0
#   $a1 = GS_MODE_PAL              = 0x03
#   $a2 = GRAPH_MODE_FIELD         = 0
#
# PAL because the only console and BIOS dump this project has are
# European -- see tools/solid-color's module doc.
#
# Note $a1 is NOT ps2sdk's GRAPH_MODE_PAL (2). That constant indexes
# graph_mode[]; what graph_set_mode passes to the syscall is that row's
# `.mode` field -- `{ 680, 37, 2560, 256, 0x03 }` -- and libgs names the
# same value GS_MODE_PAL = 0x03 directly. Passing the index was a real
# bug here until 2026-11-27; this check is what pins the distinction
# down, so don't "simplify" it back to the enum name.
#
# Still unproven, and unprovable this way: whether the screen turns red.
# That needs PCSX2 and a BIOS dump (docs/next-steps.md #1).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-qemu.sh
source "$here/lib-qemu.sh"

elf="${1:?usage: verify-solid-color-qemu.sh <path-to-elf>}"

qemu_require_tools

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

set_gs_crt_addr="$(elf_sym "$elf" set_gs_crt)"
if [[ -z "$set_gs_crt_addr" ]]; then
    echo "verify-solid-color-qemu: no \`set_gs_crt\` symbol in $elf" >&2
    exit 1
fi

work="$(mktemp -d -t verify-solid-color-qemu.XXXXXX)"
cleanup() {
    qemu_cleanup
    rm -rf "$work"
}
trap cleanup EXIT

qemu_launch "$elf"

# Note there is no `continue` past the breakpoint anywhere below: the
# next instruction retired after this point is the `syscall` that cannot
# complete, so stopping here and reading state is the whole test.
script="$work/driver.gdb"
cat > "$script" <<EOF
set confirm off
set pagination off
set height 0
target remote :$QEMU_PORT
break *$set_gs_crt_addr
continue
printf "RESULT pc 0x%x\n", \$pc
printf "RESULT a0 %d\n", \$a0
printf "RESULT a1 %d\n", \$a1
printf "RESULT a2 %d\n", \$a2
printf "RESULT next_insn_is_syscall %d\n", (*(unsigned int*)(\$pc + 4) == 0x0000000c)
printf "RESULT done 1\n"
EOF

gdb_out="$work/gdb.out"
gdb_status=0
timeout 60 gdb -batch -x "$script" "$elf" > "$gdb_out" 2>&1 || gdb_status=$?

result() { awk -v k="$1" '$1 == "RESULT" && $2 == k {print $3; exit}' "$gdb_out"; }

if [[ "$(result done)" != "1" ]]; then
    echo "FAIL  execution never reached set_gs_crt" >&2
    if (( gdb_status == 124 )); then
        echo "      (gdb timed out before the breakpoint was hit -- so the" >&2
        echo "       tool hung or faulted somewhere in boot, before the" >&2
        echo "       first thing rust_main does)" >&2
    else
        echo "      (gdb exited $gdb_status)" >&2
    fi
    echo "--- gdb output ---" >&2
    cat "$gdb_out" >&2
    exit 1
fi

check "reached set_gs_crt" \
    "$(printf '0x%x' "$((  $(result pc)  ))")" \
    "$(printf '0x%x' "$((set_gs_crt_addr))")"

# The stub's first instruction is `li $3, 2` (the syscall number); the
# `syscall` itself is the second. Asserting that keeps the register
# readings above honest about *where* execution stopped -- i.e. that
# these really are the arguments as the BIOS would see them, with
# nothing left to happen in between.
check "stopped with \`syscall\` as the next instruction" \
    "$(result next_insn_is_syscall)" "1"

check "\$a0 (interlaced = GRAPH_MODE_NONINTERLACED)" "$(result a0)" "0"
check "\$a1 (display_mode = GS_MODE_PAL, the graph_mode[].mode value)" \
    "$(result a1)" "3"
check "\$a2 (frame = GRAPH_MODE_FIELD)" "$(result a2)" "0"

if [[ "$fail" -ne 0 ]]; then
    echo "verify-solid-color-qemu: FAILED" >&2
    exit 1
fi
echo "verify-solid-color-qemu: all checks passed"
