#!/usr/bin/env bash
# Shared plumbing for the PCSX2-based checks. Not executable on its own.
#
# PCSX2 is the only thing here that is actually a PS2: real BIOS, real
# EE/GS/IOP emulation. Everything qemu cannot speak to -- the GS, BIOS
# syscalls, the EE kernel's memory map -- is only testable here. It is
# also the only layer that would have caught the ELF-class and
# proof-of-life bugs found on 2026-11-27 (docs/progress.md).
#
# The cost is a BIOS dump, which is copyrighted and cannot live in the
# repo. Drop one in bios_dump/ (gitignored) and these checks run;
# without one they skip loudly rather than failing.
#
# Three PCSX2 behaviours worth knowing, all found the hard way:
#
#   - It silently refuses to boot anything while `SetupWizardIncomplete`
#     is true. No error, no log line -- it just sits there.
#   - The ELF must be a positional argument AND end in `.elf`. The
#     `-elf <file>` flag alone boots nothing, and a file without the
#     extension is never executed (confirmed by testing both).
#   - `-nogui` implies batch mode, so PCSX2 exits when the VM stops --
#     which is how a crashed guest shows up as a dead process.

PCSX2_BIN="${PCSX2_BIN:-pcsx2-qt}"
REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
# Where to look for a BIOS dump, and where PCSX2's own state goes. Both
# gitignored; the datapath keeps PCSX2 from touching ~/.config/PCSX2.
BIOS_DIR="${BIOS_DIR:-$REPO_ROOT/bios_dump}"
PCSX2_DATA="${PCSX2_DATA:-$REPO_ROOT/.pcsx2}"

PCSX2_PID=""
PCSX2_LOG=""
PCSX2_ELF_LINK=""

pcsx2_require_tools() {
    local missing=0 tool
    for tool in "$PCSX2_BIN" nm python3; do
        if ! command -v "$tool" >/dev/null; then
            echo "${0##*/}: $tool is required but not on PATH" >&2
            missing=1
        fi
    done
    (( missing )) && exit 1
    return 0
}

# Name of the BIOS file inside BIOS_DIR, or empty if there isn't one.
pcsx2_find_bios() {
    local f
    for f in "$BIOS_DIR"/*; do
        [[ -f "$f" ]] || continue
        case "${f,,}" in
            *.bin | *.rom* ) echo "${f##*/}"; return 0 ;;
        esac
    done
    return 0
}

# Exits 0 (success) after printing a skip notice if no BIOS is present,
# so callers can `pcsx2_skip_if_no_bios || exit 0`.
pcsx2_skip_if_no_bios() {
    if [[ -z "$(pcsx2_find_bios)" ]]; then
        echo "SKIP  ${0##*/}: no PS2 BIOS dump found in $BIOS_DIR"
        echo "      These checks need one (copyrighted, not redistributable, so"
        echo "      not in the repo). Drop a dump from a console you own there"
        echo "      and re-run. Everything in 'just test-static' and"
        echo "      'just test-qemu' runs without it."
        return 1
    fi
    return 0
}

# Build a PCSX2 config that boots headlessly and exposes PINE.
pcsx2_write_config() {
    local bios="$1"
    mkdir -p "$PCSX2_DATA"

    # Always regenerate from PCSX2's defaults rather than patching
    # whatever is already there. Otherwise a setting changed by hand
    # while debugging (EnableEE, say) silently persists into every later
    # run and quietly becomes part of what the checks are testing.
    local ini="$PCSX2_DATA/PCSX2/inis/PCSX2.ini"
    rm -f "$ini"
    "$PCSX2_BIN" -datapath "$PCSX2_DATA" -testconfig >/dev/null 2>&1 || true

    if [[ ! -f "$ini" ]]; then
        echo "${0##*/}: PCSX2 did not generate $ini" >&2
        return 1
    fi

    mkdir -p "$PCSX2_DATA/PCSX2/bios"
    cp -f "$BIOS_DIR/$bios" "$PCSX2_DATA/PCSX2/bios/$bios"

    # SetupWizardIncomplete is the important one -- left true, PCSX2
    # accepts the command line and then quietly boots nothing.
    sed -i \
        -e 's|^SetupWizardIncomplete = true|SetupWizardIncomplete = false|' \
        -e 's|^EnablePINE = false|EnablePINE = true|' \
        -e "s|^BIOS = .*|BIOS = $bios|" \
        "$ini"
}

# pcsx2_launch <elf> [extra args...]
#
# Boots `elf` headlessly. The ELF is *copied* to a `.elf` name first:
# PCSX2 dispatches on the extension and our build artifacts don't have
# one. A symlink is not good enough -- PCSX2 boots garbage from one and
# ends up executing at address 0 (observed), so copy the bytes.
pcsx2_launch() {
    local elf="$1"
    shift

    PCSX2_ELF_LINK="$(mktemp -d -t pcsx2-elf.XXXXXX)/${elf##*/}.elf"
    cp -f "$elf" "$PCSX2_ELF_LINK"

    PCSX2_LOG="$PCSX2_DATA/PCSX2/logs/emulog.txt"
    rm -f "$PCSX2_LOG"

    "$PCSX2_BIN" -datapath "$PCSX2_DATA" -nogui -fastboot "$@" "$PCSX2_ELF_LINK" \
        >"${PCSX2_ELF_LINK%.elf}.stdout" 2>&1 &
    PCSX2_PID=$!
}

# Wait until PCSX2 logs that it has started executing the ELF.
pcsx2_wait_for_exec() {
    local waited=0 limit="${1:-120}" # in 0.5s ticks
    while ! grep -q "is executing" "$PCSX2_LOG" 2>/dev/null; do
        if ! kill -0 "$PCSX2_PID" 2>/dev/null; then
            echo "${0##*/}: PCSX2 exited before executing the ELF." >&2
            echo "      This is what an ELF its loader cannot parse looks like" >&2
            echo "      (an ELF64 image crashes it outright -- see" >&2
            echo "      docs/progress.md 2026-11-27)." >&2
            [[ -f "$PCSX2_LOG" ]] && tail -20 "$PCSX2_LOG" >&2
            return 1
        fi
        if (( waited >= limit )); then
            echo "${0##*/}: PCSX2 never started executing the ELF" >&2
            [[ -f "$PCSX2_LOG" ]] && tail -20 "$PCSX2_LOG" >&2
            return 1
        fi
        sleep 0.5
        waited=$(( waited + 1 ))
    done
}

pcsx2_cleanup() {
    if [[ -n "$PCSX2_PID" ]] && kill -0 "$PCSX2_PID" 2>/dev/null; then
        kill "$PCSX2_PID" 2>/dev/null || true
        wait "$PCSX2_PID" 2>/dev/null || true
    fi
    [[ -n "$PCSX2_ELF_LINK" ]] && rm -rf "$(dirname "$PCSX2_ELF_LINK")"
}

# Address of a symbol, 0x-prefixed; empty if absent.
pcsx2_elf_sym() {
    local elf="$1" sym="$2" addr
    addr="$(nm "$elf" | awk -v s="$sym" '$3 == s {print $1; exit}')"
    [[ -n "$addr" ]] && echo "0x$addr"
    return 0
}
