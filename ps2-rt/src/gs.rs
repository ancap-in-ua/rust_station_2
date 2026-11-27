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

/// One row of ps2sdk's `graph_mode[]` table (`ee/graph/src/
/// graph_mode.c`): the display geometry for a video standard, plus the
/// value its `SetGsCrt` call passes.
///
/// Kept as a table rather than as loose constants specifically because
/// of how these got mixed up before: `crt_mode` is the row's `.mode`
/// field, **not** `graph.h`'s `GRAPH_MODE_NTSC`/`_PAL`, which are
/// indices *into* this table. Reading the geometry out of one row while
/// taking the mode number from the enum next to it is exactly the bug
/// that shipped on 2026-11-27 (see docs/progress.md). Bundling them
/// makes that mismatch impossible.
pub struct VideoMode {
    /// Display region origin in the output signal's coordinate space.
    pub dx: u16,
    pub dy: u16,
    /// Display region size in output pixels/lines (pre-magnification).
    pub dw: u16,
    pub dh: u16,
    /// `SetGsCrt`'s `display_mode`. Same values libgs calls
    /// `GS_MODE_NTSC`/`GS_MODE_PAL`.
    pub crt_mode: i16,
}

/// `graph_mode[GRAPH_MODE_NTSC]` -- `{ 652, 26, 2560, 224, 0x02 }`.
pub const NTSC_NONINTERLACED: VideoMode = VideoMode {
    dx: 652,
    dy: 26,
    dw: 2560,
    dh: 224,
    crt_mode: 0x02,
};

/// `graph_mode[GRAPH_MODE_PAL]` -- `{ 680, 37, 2560, 256, 0x03 }`.
pub const PAL_NONINTERLACED: VideoMode = VideoMode {
    dx: 680,
    dy: 37,
    dw: 2560,
    dh: 256,
    crt_mode: 0x03,
};

/// Brings the PCRTC up for `mode` with a `width`-pixel-wide PSMCT32
/// framebuffer at VRAM offset 0, following ps2sdk's `graph_set_mode`
/// order: GS reset, interrupt mask, `SetGsCrt`, then SMODE2 and the
/// display/framebuffer registers.
///
/// Leaves PMODE and BGCOLOR alone -- what to actually put on screen is
/// the caller's business.
///
/// # Safety
/// Same as [`write_reg`], plus: this resets the GS, so nothing else may
/// be using it.
#[cfg(target_arch = "mips")]
pub unsafe fn init_display(mode: &VideoMode, width: u16) {
    // graph_set_screen's formula: magh = dw/width, magv = dh/height,
    // and the register field is the factor minus one (0 = 1x).
    let magh = (mode.dw / width - 1) as u8;
    let magv = 0; // height == mode.dh here, so magv = 1x.

    unsafe {
        write_reg(REG_CSR, CSR_RESET);
        write_reg(REG_CSR, 0);
        write_reg(REG_IMR, IMR_MASK_ALL);

        crate::syscall::set_gs_crt(
            crate::syscall::NON_INTERLACED,
            mode.crt_mode,
            crate::syscall::FIELD,
        );

        write_reg(REG_SMODE2, smode2(false, false, 0));
        // graph_set_framebuffer's formula: fbp >> 11 (0 here), width >> 6.
        write_reg(REG_DISPFB1, dispfb(0, (width / 64) as u8, PSMCT32, 0, 0));
        write_reg(
            REG_DISPLAY1,
            display(mode.dx, mode.dy, magh, magv, mode.dw - 1, mode.dh - 1),
        );
    }
}

/// `PSM` encoding for 32-bit RGBA.
pub const PSMCT32: u8 = 0;

/// Writes a GS privileged register. `addr` must be one of the `REG_*`
/// constants above (or another address in the privileged range) and the
/// write must be a single 64-bit store -- these registers are not
/// byte/word addressable.
///
/// The `sd` is hand-written because `core::ptr::write_volatile::<u64>`
/// does **not** produce one on this target. Under o32 (32-bit pointers)
/// LLVM lowers a volatile 64-bit store as two 32-bit `sw`s to `addr`
/// and `addr + 4`:
///
/// ```text
/// sw $2, 0x0($1)      # $1 = 0x12001000 -- CSR low word
/// sw $zero, 0x0($2)   # $2 = 0x12001004 -- CSR high word
/// ```
///
/// which is precisely the access pattern the register's documentation
/// rules out. It worked by accident while this was an n64/ELF64 target,
/// where LLVM had a 64-bit store to emit; re-targeting to o32 on
/// 2026-11-27 silently split every GS register write in the project.
/// ps2sdk gets `sd` here because ee-gcc emits one for its `volatile u64
/// *` writes even under o32 -- the R5900 has 64-bit stores regardless
/// of pointer width, LLVM just won't use them for this.
///
/// The `dsll32`/`dsrl32` dance assembles the two 32-bit halves into one
/// 64-bit register: Rust's `reg` class is 32-bit on this target, so
/// there is no way to hand `asm!` a `u64` directly. `lo` is
/// zero-extended (a 32-bit value in a MIPS64 register is
/// *sign*-extended, which would corrupt the upper half) and `hi` is
/// shifted into place.
///
/// # Safety
/// Caller must ensure this runs in EE kernel mode (privileged-region
/// accesses from user mode raise an Address Error exception) and that
/// writing the given register with the given value is sound for whatever
/// display state currently exists.
#[cfg(target_arch = "mips")]
pub unsafe fn write_reg(addr: *mut u64, value: u64) {
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    unsafe {
        core::arch::asm!(
            // The register allocator is free to hand us $at, and the
            // assembler warns about touching it outside `.set noat`
            // (same reason boot.rs's _start brackets its $gp setup).
            // Every instruction here is a real one, not a macro the
            // assembler would need $at to expand, so this is just
            // telling it we meant it.
            ".set noat",
            "dsll32 {lo}, {lo}, 0",
            "dsrl32 {lo}, {lo}, 0",
            "dsll32 {hi}, {hi}, 0",
            "or     {lo}, {lo}, {hi}",
            "sd     {lo}, 0({addr})",
            ".set at",
            lo = inout(reg) lo => _,
            hi = inout(reg) hi => _,
            addr = in(reg) addr,
            options(nostack, preserves_flags),
        );
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
