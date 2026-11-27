//! next-steps.md #2's "get a solid color on screen" milestone, attempted
//! via the PMODE-blend shortcut from docs/progress.md 2026-11-26 instead
//! of a GIF/DMA primitive draw: force the PCRTC's alpha blend to take
//! 100% of its color from `BGCOLOR` (`ALP = 0` with `MMOD`/`SLBG` set,
//! see `ps2_rt::gs`'s module doc for the confirmed-against-primary-source
//! blend equation), so the framebuffer's actual contents never matter.
//!
//! "The screen actually turns red" is UNCONFIRMED and **cannot be
//! confirmed on PCSX2** — not because of anything wrong here, but
//! because PCSX2 does not emulate this path. Its merge circuit bails
//! before BGCOLOR is ever applied when no read circuit has a real
//! framebuffer texture, which is precisely our situation (we never
//! write a pixel to VRAM). From `GSRenderer::Merge`:
//!
//! ```text
//! if (!tex[0] && !tex[1])
//! {
//!     ...
//!     // Both circuits off still outputs BGCOLOR on real hardware.
//!     if (PCRTCDisplays[0].enabled || PCRTCDisplays[1].enabled)
//!     {
//!         m_real_size = GSVector2i(0, 0);
//!         return false;
//!     }
//! }
//! ```
//!
//! PCSX2's own comment concedes the divergence. Verified empirically
//! too: black with `EN1 = 1` (the configuration below) and black with
//! both circuits disabled. So this tool runs correctly end to end on
//! PCSX2 — syscall returns, every register written, proof-of-life
//! published — and still shows nothing, and that tells us nothing about
//! hardware. Settling it needs a real console; see docs/next-steps.md
//! #2.
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
// Non-interlaced, field mode, DPMS on -- the same three values handed to
// set_gs_crt, which is what ps2sdk keeps them consistent with.
const SMODE2_VALUE: u64 = gs::smode2(false, false, 0);

const _: () = assert!(PMODE_VALUE == 0xa5);
const _: () = assert!(SMODE2_VALUE == 0x0);

/// Published once every register write below has happened.
const PROOF_OF_LIFE_VALUE: u32 = 0x5011_D000;

#[no_mangle]
extern "C" fn rust_main() -> ! {
    unsafe {
        // Order and contents follow ps2sdk's own bring-up
        // (`graph_set_mode` -> `graph_set_screen` ->
        // `graph_set_framebuffer` -> `graph_set_bgcolor`), rather than
        // the "just write the four interesting registers" sequence this
        // tool used until 2026-11-27. Everything before `set_gs_crt`
        // here was missing, and SMODE2 was never written at all.
        gs::write_reg(gs::REG_CSR, gs::CSR_RESET);
        gs::write_reg(gs::REG_CSR, 0);
        gs::write_reg(gs::REG_IMR, gs::IMR_MASK_ALL);

        syscall::set_gs_crt(syscall::NON_INTERLACED, syscall::NTSC, syscall::FIELD);

        gs::write_reg(gs::REG_SMODE2, SMODE2_VALUE);
        gs::write_reg(gs::REG_DISPFB1, DISPFB1_VALUE);
        gs::write_reg(gs::REG_DISPLAY1, DISPLAY1_VALUE);
        gs::write_reg(gs::REG_BGCOLOR, BGCOLOR_VALUE);
        gs::write_reg(gs::REG_PMODE, PMODE_VALUE);
    }
    // Reaching here means the whole sequence ran -- it does, on real
    // PCSX2. That is what separates "the register writes never happened"
    // from "they happened and the screen still isn't red", which are
    // very different problems and were confused for each other once
    // already (see docs/progress.md 2026-11-27).
    ps2_rt::set_proof(PROOF_OF_LIFE_VALUE);
    ps2_rt::ps2_rt_halt()
}
