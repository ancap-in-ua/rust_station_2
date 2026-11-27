#!/usr/bin/env bash
# Shared plumbing for the qemu-based dynamic checks (verify-qemu.sh,
# verify-solid-color-qemu.sh, run-qemu.sh). Not executable on its own.
#
# ---------------------------------------------------------------------
# Why qemu at all, when the target is a PS2?
#
# Everything in this repo was, until this existed, verified *statically*
# only -- no code had ever been executed anywhere, because booting it
# needs PCSX2 with a legally-obtained BIOS dump that this environment
# doesn't have (docs/next-steps.md #1). qemu needs no BIOS, so it closes
# most of that gap: `_start`'s hand-written assembly, the `$gp`/`$sp`
# setup, bss zeroing, the bump allocator, and -- most importantly --
# whether code generated from our hand-written target spec actually
# *runs* are all CPU/RAM-level facts that a generic MIPS64 emulator can
# settle.
#
# What makes an unmodified PS2 ELF loadable here at all: link.ld puts
# everything at 0x00100000 with PhysAddr == VirtAddr, which the PS2's
# loaders treat as identity-mapped physical RAM -- and that is also
# exactly how a MIPS CPU behaves at reset, where `Status.ERL = 1` makes
# kuseg unmapped and identity-mapped to physical addresses. So the same
# ELF, byte for byte, works in both places for the same reason.
#
# What this deliberately does NOT prove, and can't:
#
#   - Anything GS/PCRTC. qemu has no PS2 graphics hardware, so writes to
#     0x1200_0000 go nowhere observable. `solid-color`'s actual claim
#     ("the screen turns red") is still unverified -- see
#     verify-solid-color-qemu.sh for the part of it that *is* checkable.
#   - Anything BIOS. `syscall` traps to an exception vector that holds no
#     handler here, so `ps2_rt::syscall::set_gs_crt` cannot complete
#     (confirmed by hand: stepping over the `syscall` instruction hangs).
#   - Anything R5900-specific. The EE's 128-bit MMI instructions, its
#     non-IEEE-754 single-precision FPU, its scratchpad RAM and cache
#     behaviour are all absent from qemu's generic MIPS models. Code that
#     relies on them would pass here and fail on hardware.
#
# So: `-cpu R4000`, chosen because it's the closest thing qemu models to
# the R5900's baseline -- a 64-bit MIPS III part, matching the `mips3`
# ELF flag that verify-elf.sh asserts. (`5Kc`, MIPS64r1, was also tried
# and behaves identically on these tools; R4000 is the more honest
# choice since it can't quietly accept a post-MIPS-III instruction that
# the EE would reject.)
#
# `-M malta` is a plain MIPS board used only for its RAM; `-m 32` matches
# the PS2's 32 MiB exactly, so link.ld's `_stack_top = 0x2000000` sits at
# the real end of RAM rather than somewhere forgiving. Note that malta's
# own `-kernel` path can't be used: it rejects anything linked below
# 0x80000000 ("Trap-and-Emul kernels ... are not supported"), which is
# every PS2 ELF. `-device loader` loads the segments and sets the CPU's
# PC from the ELF entry without that check.

QEMU_BIN="${QEMU_BIN:-qemu-system-mips64el}"
QEMU_MACHINE_ARGS=(-M malta -cpu R4000 -m 32)

# Set by qemu_launch, read by qemu_cleanup / callers.
QEMU_PID=""
QEMU_PORT=""
QEMU_LOG=""

qemu_require_tools() {
    local missing=0 tool
    for tool in "$QEMU_BIN" gdb nm; do
        if ! command -v "$tool" >/dev/null; then
            echo "${0##*/}: $tool is required but not on PATH" >&2
            missing=1
        fi
    done
    (( missing )) && exit 1
    return 0
}

# Is something already listening on this TCP port? Read straight out of
# /proc rather than shelling out to `ss`/`lsof` (not guaranteed in the
# dev shell), and deliberately *without* connecting: qemu's gdbstub
# treats a connect-then-close as a debugger detaching, which resumes the
# guest -- which would defeat the whole point of `-S`, since callers need
# the guest still frozen at the entry point so they can set memory up
# before a single instruction runs.
qemu_port_listening() {
    local hexport
    hexport="$(printf '%04X' "$1")"
    grep -qiE "^ *[0-9]+: *[0-9A-F]+:$hexport +[0-9A-F]+:[0-9A-F]+ +0A" \
        /proc/net/tcp /proc/net/tcp6 2>/dev/null
}

qemu_pick_port() {
    local port attempt
    for attempt in $(seq 1 50); do
        port=$(( 20000 + RANDOM % 30000 ))
        if ! qemu_port_listening "$port"; then
            echo "$port"
            return 0
        fi
    done
    echo "${0##*/}: could not find a free TCP port for qemu's gdbstub" >&2
    return 1
}

# qemu_launch <elf> [extra qemu args...]
#
# Boots `elf` with the guest frozen at its entry point (`-S`) and a
# gdbstub listening on QEMU_PORT, then waits until that port is actually
# accepting connections before returning -- otherwise a fast caller races
# qemu's startup and gdb fails with "connection refused".
qemu_launch() {
    local elf="$1"
    shift

    QEMU_PORT="$(qemu_pick_port)" || return 1
    QEMU_LOG="$(mktemp -t qemu-log.XXXXXX)"

    "$QEMU_BIN" "${QEMU_MACHINE_ARGS[@]}" \
        -device "loader,file=$elf,cpu-num=0" \
        -display none -serial none \
        -S -gdb "tcp::$QEMU_PORT" \
        "$@" >"$QEMU_LOG" 2>&1 &
    QEMU_PID=$!

    local waited=0
    while ! qemu_port_listening "$QEMU_PORT"; do
        if ! kill -0 "$QEMU_PID" 2>/dev/null; then
            echo "${0##*/}: qemu exited before its gdbstub came up:" >&2
            cat "$QEMU_LOG" >&2
            return 1
        fi
        if (( waited >= 200 )); then # 200 * 0.05s = 10s
            echo "${0##*/}: qemu's gdbstub never started listening on port $QEMU_PORT" >&2
            cat "$QEMU_LOG" >&2
            return 1
        fi
        sleep 0.05
        waited=$(( waited + 1 ))
    done
}

qemu_cleanup() {
    if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
        kill "$QEMU_PID" 2>/dev/null || true
        wait "$QEMU_PID" 2>/dev/null || true
    fi
    [[ -n "$QEMU_LOG" ]] && rm -f "$QEMU_LOG"
}

# Address of a symbol in an ELF's symbol table, as a 0x-prefixed value;
# empty (success) if the symbol isn't present, so callers can treat
# individual symbols as optional (`ps2_rt_panicked` is garbage-collected
# out of tools with no reachable panic path).
elf_sym() {
    local elf="$1" sym="$2" addr
    addr="$(nm "$elf" | awk -v s="$sym" '$3 == s {print $1; exit}')"
    [[ -n "$addr" ]] && echo "0x$addr"
    return 0
}
