//! next-steps.md #2's "get a solid color on screen" milestone, attempted
//! via the PMODE-blend shortcut from docs/progress.md 2026-11-26 instead
//! of a GIF/DMA primitive draw: force the PCRTC's alpha blend to take
//! 100% of its color from `BGCOLOR` (`ALP = 0` with `MMOD`/`SLBG` set,
//! see `ps2_rt::gs`'s module doc for the confirmed-against-primary-source
//! blend equation), so the framebuffer's actual contents never matter.
//!
//! NOT dynamically verified -- no BIOS dump available in this
//! environment to boot this under PCSX2/hardware (see docs/next-steps.md
//! #1). Everything here compiles and the register values are sourced
//! (see below), but "the screen actually turns red" is unconfirmed.
//!
//! Display geometry (`DX`/`DY`/`DW`/`DH`/`MAGH`/`MAGV`/`FBW`) is not
//! hand-derived: these are ps2sdk's own `graph_mode[GRAPH_MODE_NTSC]`
//! preset (`{ x: 652, y: 26, width: 2560, height: 224 }`,
//! `ee/graph/src/graph_mode.c`) run through its documented
//! `graph_set_screen`/`graph_set_framebuffer` formulas for a
//! non-interlaced 640x224 PSMCT32 screen at VRAM offset 0 -- a
//! known-working reference, not a guess:
//!   - `DISPFB1`: `fbp=0>>11=0`, `fbw=640>>6=10`, `psm=0` (PSMCT32).
//!   - `DISPLAY1`: `magh = 2560/640 = 4` -> field value `magh-1=3`;
//!     `magv = 224/224 = 1` -> field value `magv-1=0`; `dw = 2560-1`;
//!     `dh = 224-1`; `dx=652`, `dy=26` unchanged (non-interlaced, no x/y
//!     offset requested).

#![no_std]
#![no_main]

use ps2_rt::gs;
use ps2_rt::syscall;

use ps2_rt as _; // pulls in the panic handler and _start

// ps2sdk's graph_mode[GRAPH_MODE_NTSC] preset (ee/graph/src/graph_mode.c):
// { x: 652, y: 26, width: 2560, height: 224 }, for a non-interlaced
// 640x224 screen with no x/y offset requested (graph_x = graph_y = 0).
const NTSC_DX: u16 = 652;
const NTSC_DY: u16 = 26;
const SCREEN_WIDTH: u16 = 640;
const SCREEN_HEIGHT: u16 = 224;
const NTSC_DW: u16 = 2560; // graph_mode[NTSC].width
const NTSC_DH: u16 = 224; // graph_mode[NTSC].height (== SCREEN_HEIGHT here)

// graph_set_screen's formula: magh = dw/width, magv = dh/height, and the
// DISPLAY register field is magh-1/magv-1 (0 = x1).
const MAGH_FIELD: u8 = (NTSC_DW / SCREEN_WIDTH - 1) as u8;
const MAGV_FIELD: u8 = (NTSC_DH / SCREEN_HEIGHT - 1) as u8;

// VRAM offset 0, PSMCT32 -- graph_set_framebuffer's formula: fbp>>11,
// width>>6.
const FRAME_BASE: u16 = 0;
const BUFFER_WIDTH_FIELD: u8 = (SCREEN_WIDTH / 64) as u8;
const PSMCT32: u8 = 0;

const DISPFB1_VALUE: u64 = gs::dispfb(FRAME_BASE, BUFFER_WIDTH_FIELD, PSMCT32, 0, 0);
const DISPLAY1_VALUE: u64 = gs::display(
    NTSC_DX,
    NTSC_DY,
    MAGH_FIELD,
    MAGV_FIELD,
    NTSC_DW - 1,
    NTSC_DH - 1,
);
// Red. BGCOLOR's RGB order (R low byte) is confirmed against the primary
// source -- see gs.rs.
const BGCOLOR_VALUE: u64 = gs::bgcolor(0xff, 0x00, 0x00);
// EN1 on (feeds the merge circuit; its own pixel data is irrelevant
// since ALP=0 below), EN2 off (unused, circuit 2's output is never
// selected), MMOD on (use the fixed ALP value instead of circuit 1's
// per-pixel alpha), SLBG on (blend against BGCOLOR instead of circuit
// 2), ALP=0 (0% circuit 1, 100% BGCOLOR -- see the Merge Circuit blend
// equation in gs.rs).
const PMODE_VALUE: u64 = gs::pmode(true, false, true, false, true, 0);

// These numbers are what scripts/verify-solid-color.sh checks ended up
// in the built ELF (see that script for why checking the *computed*
// register values here, rather than the compiled instruction bytes that
// load them, is the more regression-resistant place to pin them down).
const _: () = assert!(DISPFB1_VALUE == 0x1400);
const _: () = assert!(DISPLAY1_VALUE == 0x000d_f9ff_0181_a28c);
const _: () = assert!(BGCOLOR_VALUE == 0xff);
const _: () = assert!(PMODE_VALUE == 0xa5);

#[no_mangle]
extern "C" fn rust_main() -> ! {
    unsafe {
        syscall::set_gs_crt(syscall::NON_INTERLACED, syscall::NTSC, syscall::FIELD);
        gs::write_reg(gs::REG_DISPFB1, DISPFB1_VALUE);
        gs::write_reg(gs::REG_DISPLAY1, DISPLAY1_VALUE);
        gs::write_reg(gs::REG_BGCOLOR, BGCOLOR_VALUE);
        gs::write_reg(gs::REG_PMODE, PMODE_VALUE);
    }
    loop {}
}
