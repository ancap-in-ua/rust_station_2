//! GS privileged registers (PCRTC: the display-output side of the Graphics
//! Synthesizer), addresses and bit layouts only -- no driving logic yet.
//!
//! Source for every address and bit field below: the primary source,
//! Sony's GS User's Manual Version 6.0 ("Privileged Registers" address
//! list in section 7.3, and the per-register `BIT ASSIGN`/`FIELD` tables
//! for `PMODE`, `BGCOLOR`, `DISPFB1/2`, `DISPLAY1/2` -- `0x00`/`0x0e`/
//! `0x07`/`0x08` in GS-register-address terms, `0x1200_0000`/`0x1200_00e0`/
//! `0x1200_0070`/`0x1200_0080` in EE-physical-address terms). Cross-checked
//! against ps2sdk's `common/include/gs_privileged.h` (`GS_REG_*`/
//! `GS_SET_*`) and ps2tek's (<https://psi-rockin.github.io/ps2tek/>)
//! privileged-register address table -- all three agree. See
//! docs/progress.md 2026-11-26.
//!
//! These are plain memory-mapped registers, not GS *drawing* state (that
//! goes through the GIF via DMA/direct FIFO writes and is unimplemented
//! here -- see docs/next-steps.md #2). They also don't cover video
//! sync/clock timing (`SMODE1`/`SYNCH1`/`SYNCH2`/`SYNCV`/`SRFSH`) --
//! the primary source documents those as configured by a BIOS kernel
//! service rather than as registers application code hand-derives
//! values for; see `ps2_rt::syscall::set_gs_crt`.
//!
//! No address translation needed: the EE's virtual/physical memory map
//! has this entire region (0x10000000-0x13FFFFFF: I/O registers, VU0/VU1
//! memory, GS privileged registers) identity-mapped (virtual == physical)
//! and accessible directly from kuseg, unlike main RAM's kseg0/kseg1
//! cached/uncached aliases -- confirmed against ps2tek's EE memory map
//! table, not assumed. See docs/progress.md 2026-11-26.

/// PCRTC mode setting: which read circuits are enabled and how their
/// output is blended into the final display signal.
pub const REG_PMODE: *mut u64 = 0x1200_0000 as *mut u64;
/// Video synchronization mode (interlace, field mode, display enable).
pub const REG_SMODE2: *mut u64 = 0x1200_0020 as *mut u64;
/// Framebuffer read by output circuit 1.
pub const REG_DISPFB1: *mut u64 = 0x1200_0070 as *mut u64;
/// Display region/timing for output circuit 1.
pub const REG_DISPLAY1: *mut u64 = 0x1200_0080 as *mut u64;
/// Framebuffer read by output circuit 2.
pub const REG_DISPFB2: *mut u64 = 0x1200_0090 as *mut u64;
/// Display region/timing for output circuit 2.
pub const REG_DISPLAY2: *mut u64 = 0x1200_00a0 as *mut u64;
/// Background color used where PMODE.SLBG selects it instead of circuit
/// 2's output.
pub const REG_BGCOLOR: *mut u64 = 0x1200_00e0 as *mut u64;
/// System status, and the GS reset control. Lives in the second
/// privileged page (`0x1200_1000`) rather than alongside the registers
/// above.
pub const REG_CSR: *mut u64 = 0x1200_1000 as *mut u64;
/// Interrupt mask. Written before bringing the display up so the GS
/// doesn't raise interrupts nothing is set up to handle.
pub const REG_IMR: *mut u64 = 0x1200_1010 as *mut u64;

/// Writing this to [`REG_CSR`] resets the GS. ps2sdk's `graph_set_mode`
/// does this (`*GS_REG_CSR = (u64)1 << 9;`) followed by a zeroing write,
/// before calling `SetGsCrt` -- it is the first thing in its display
/// bring-up sequence.
pub const CSR_RESET: u64 = 1 << 9;

/// Masks every GS interrupt source. The value ps2sdk's `graph_set_mode`
/// passes to `GsPutIMR` during bring-up.
pub const IMR_MASK_ALL: u64 = 0x0000_7700;

/// Writes a GS privileged register. `addr` must be one of the `REG_*`
/// constants above (or another address in the privileged range) and the
/// write must be a single 64-bit store -- these registers are not
/// byte/word addressable.
///
/// # Safety
/// Caller must ensure this runs in EE kernel mode (privileged-region
/// accesses from user mode raise an Address Error exception) and that
/// writing the given register with the given value is sound for whatever
/// display state currently exists.
pub unsafe fn write_reg(addr: *mut u64, value: u64) {
    unsafe {
        core::ptr::write_volatile(addr, value);
    }
}

/// Packs PMODE's fields. `en1`/`en2` enable read circuits 1/2;
/// `use_fixed_alpha` selects a fixed blend factor (`alpha`, 0-255, 0xff =
/// 1.0) instead of circuit 1's per-pixel alpha; `out1_alpha_from_out2`
/// controls the EN1-disabled alpha-output path; `select_bgcolor` blends
/// against `BGCOLOR` instead of circuit 2's output. CRTMD is hardwired to
/// 1 (the only value ps2sdk ever sets, and that's documented, not just
/// copied — see gs_privileged.h: "CRTMD is always set 1").
pub const fn pmode(
    en1: bool,
    en2: bool,
    use_fixed_alpha: bool,
    out1_alpha_from_out2: bool,
    select_bgcolor: bool,
    alpha: u8,
) -> u64 {
    const CRTMD: u64 = 1;
    (en1 as u64)
        | (en2 as u64) << 1
        | (CRTMD & 0x7) << 2
        | (use_fixed_alpha as u64) << 5
        | (out1_alpha_from_out2 as u64) << 6
        | (select_bgcolor as u64) << 7
        | (alpha as u64) << 8
}

/// Packs SMODE2's fields. `interlace` and `frame_mode` mirror the
/// corresponding `set_gs_crt` arguments; `dpms` is the VESA power state
/// (0 = on).
///
/// `SetGsCrt` is documented as setting the video *timing* registers, but
/// not this one -- ps2sdk's `GS_SET_SMODE2(INT, FFMD, DPMS)` exists
/// separately for exactly that reason, and SMODE2 carries the bit that
/// decides whether anything reaches the CRTC at all.
pub const fn smode2(interlace: bool, frame_mode: bool, dpms: u8) -> u64 {
    (interlace as u64) | (frame_mode as u64) << 1 | (dpms as u64 & 0x3) << 2
}

/// Packs BGCOLOR's R/G/B fields (8 bits each, no alpha -- PMODE.ALP
/// supplies the blend factor separately).
pub const fn bgcolor(r: u8, g: u8, b: u8) -> u64 {
    (r as u64) | (g as u64) << 8 | (b as u64) << 16
}

/// Packs DISPFB1/DISPFB2's fields. `frame_base` is in units of 2048
/// words, `buffer_width` in units of 64 pixels (both match `FRAME_1/2`'s
/// units, since the display circuits and the draw pipe read the same
/// framebuffer). `pixel_format` matches GS's `PSM` encoding (0 =
/// PSMCT32).
pub const fn dispfb(frame_base: u16, buffer_width: u8, pixel_format: u8, dbx: u16, dby: u16) -> u64 {
    (frame_base as u64 & 0x1ff)
        | (buffer_width as u64 & 0x3f) << 9
        | (pixel_format as u64 & 0x1f) << 15
        | (dbx as u64 & 0x7ff) << 32
        | (dby as u64 & 0x7ff) << 43
}

/// Packs DISPLAY1/DISPLAY2's fields. `dx`/`dy` are the display region's
/// top-left in the output signal's coordinate space (not framebuffer
/// pixels); `mag_h`/`mag_v` are magnification factors minus one (0 = 1x);
/// `width`/`height` are the display region's size in output pixels/lines
/// minus one.
pub const fn display(dx: u16, dy: u16, mag_h: u8, mag_v: u8, width: u16, height: u16) -> u64 {
    (dx as u64 & 0xfff)
        | (dy as u64 & 0x7ff) << 12
        | (mag_h as u64 & 0xf) << 23
        | (mag_v as u64 & 0x3) << 27
        | (width as u64 & 0xfff) << 32
        | (height as u64 & 0x7ff) << 44
}

// Bit positions themselves are now checked against the primary source
// (GS User's Manual's BIT ASSIGN/FIELD tables, see the module doc
// comment) by hand, not against hardware -- no BIOS dump available in
// this environment to boot anything that could confirm these registers
// actually drive the PCRTC the way the manual says they do. Regressions
// in the *arithmetic* below (a wrong shift/mask creeping in later) are
// still worth catching now; see docs/next-steps.md for what's still
// unverified.
// CRTMD is hardwired to 1, contributing 1 << 2 = 4 to every value below
// -- included explicitly rather than folded into "baseline" so a future
// edit can't silently drop it without a visible assertion failure.
const _: () = assert!(pmode(false, false, false, false, false, 0) == 0b0000_0100);
const _: () = assert!(pmode(true, false, false, false, false, 0) == 0b0000_0101);
const _: () = assert!(pmode(true, false, true, false, true, 0) == 0b1010_0101);
const _: () = assert!(pmode(false, false, false, false, false, 0xff) == 0xff04);
const _: () = assert!(bgcolor(0, 0, 0) == 0);
const _: () = assert!(bgcolor(0xff, 0, 0) == 0x0000ff);
const _: () = assert!(bgcolor(0, 0xff, 0) == 0x00ff00);
const _: () = assert!(bgcolor(0, 0, 0xff) == 0xff0000);
const _: () = assert!(dispfb(0, 0, 0, 0, 0) == 0);
const _: () = assert!(dispfb(1, 0, 0, 0, 0) == 1);
const _: () = assert!(dispfb(0, 1, 0, 0, 0) == 1 << 9);
const _: () = assert!(dispfb(0, 0, 1, 0, 0) == 1 << 15);
const _: () = assert!(display(0, 0, 0, 0, 0, 0) == 0);
const _: () = assert!(display(1, 0, 0, 0, 0, 0) == 1);
const _: () = assert!(display(0, 0, 0, 0, 1, 0) == 1 << 32);
