//! BIOS EE kernel syscalls.
//!
//! So far, just the one needed to unblock next-steps.md #2: `SetGsCrt`,
//! which initializes the PCRTC's video-clock/sync-timing registers
//! (`SMODE1`, `SYNCH1`, `SYNCH2`, `SYNCV`, `SRFSH`) for a given video
//! standard. Those registers are deliberately *not* exposed as
//! memory-mapped constants in `ps2_rt::gs` next to `PMODE`/`DISPFB`/etc:
//! the GS User's Manual's "Video Clock" section (5.2.1) says outright
//! that "the VCK frequency predetermined for each video mode is set by
//! the kernel service" rather than giving register values for
//! application code to hand-derive -- so this goes through that service
//! instead of guessing at the timing constants.
//!
//! Source for the syscall number, calling convention, and raw assembly
//! stub: ps2tek's BIOS EE Syscalls reference
//! (<https://psi-rockin.github.io/ps2tek/>, `02h void SetGsCrt(bool
//! interlaced, int display_mode, bool frame)`) and ps2sdk's
//! `ee/kernel/src/kernel.S` (the `SYSCALL_SPECIAL` macro's non-kmode
//! expansion: `li $3, N; syscall; jr $31; nop` -- args already sit in
//! $a0-$a2 by the normal calling convention, so the stub only needs to
//! supply the syscall number and trap) and
//! `ee/kernel/include/syscallnr.h` (`__NR_SetGsCrt` = 2). Mode constants
//! cross-checked against `ee/graph/include/graph.h`'s
//! `GRAPH_MODE_NTSC`/`_PAL`/`_INTERLACED`/`_NONINTERLACED`/`_FIELD`/
//! `_FRAME` and their use in `ee/graph/src/graph_mode.c`'s
//! `SetGsCrt(graph_interlace, graph_mode[mode].mode, graph_ffmd)` call.

use core::arch::global_asm;

#[cfg(target_arch = "mips")]
global_asm!(
    r#"
    .section .text.syscall, "ax", @progbits
    .global set_gs_crt
    .set noreorder
set_gs_crt:
    li    $3, 2
    syscall
    jr    $31
    nop
    .set reorder
    "#
);

#[cfg(target_arch = "mips")]
unsafe extern "C" {
    /// BIOS syscall 0x02. `interlace`/`field` are booleans in the same
    /// i16-wide argument slots ps2sdk's `extern void SetGsCrt(s16
    /// interlace, s16 pal_ntsc, s16 field);` prototype declares them
    /// with; `mode` is one of `NTSC`/`PAL`.
    ///
    /// # Safety
    /// Caller must ensure this runs in EE kernel/supervisor mode (a
    /// `syscall` instruction traps there regardless, but the BIOS
    /// syscall table must still be intact and unpatched for this to do
    /// anything sane) and that reinitializing the PCRTC is valid for
    /// whatever display state currently exists.
    pub fn set_gs_crt(interlace: i16, mode: i16, field: i16);
}

/// `display_mode` values for `set_gs_crt`.
///
/// These are **not** ps2sdk's `GRAPH_MODE_NTSC`/`_PAL` (1 and 2). Those
/// are indices into `graph_mode[]`, and what `graph_set_mode` actually
/// passes to the syscall is that row's `.mode` field:
///
/// ```text
/// { 652, 26, 2560, 224, 0x02 }, // NTSC-NI   <- graph_mode[GRAPH_MODE_NTSC]
/// { 680, 37, 2560, 256, 0x03 }, // PAL-NI
/// ```
///
/// i.e. `SetGsCrt(interlace, graph_mode[GRAPH_MODE_NTSC].mode, ffmd)`
/// passes 2, not 1. Cross-checked against ps2sdk's libgs, which names
/// the same values directly: `GS_MODE_NTSC = 0x02`, `GS_MODE_PAL =
/// 0x03`.
///
/// This was wrong (1 and 2, the indices) until 2026-11-27, and it is
/// why `tools/solid-color` produced a black screen on real hardware:
/// the geometry constants were read correctly out of that same table
/// row while the mode value was taken from the enum next to it. See
/// docs/progress.md.
pub const NTSC: i16 = 0x02;
pub const PAL: i16 = 0x03;

/// `interlace` values for `set_gs_crt`.
pub const NON_INTERLACED: i16 = 0;
pub const INTERLACED: i16 = 1;

/// `field` values for `set_gs_crt` -- only meaningful when interlaced.
pub const FIELD: i16 = 0;
pub const FRAME: i16 = 1;

// No const-assertable arithmetic here (unlike gs.rs's bit-packing
// helpers) -- these are just named magic numbers copied from a source,
// not computed from anything independent a const fn could cross-check.
// Real test coverage for this module is the ELF-structural check in
// scripts/verify-elf.sh once something actually calls `set_gs_crt` (see
// tools/solid-color) -- a self-referential `assert!(NTSC == 1)` right
// next to `pub const NTSC: i16 = 1` wouldn't catch anything a typo in
// *both* places couldn't also slip past.
