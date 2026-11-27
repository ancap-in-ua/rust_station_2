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
//! PAL, not NTSC: the only PS2 this project has access to (and the only
//! BIOS dump) is European. Running an NTSC signal into a PAL console
//! would make a black screen ambiguous — "the approach doesn't work"
//! vs. "the TV didn't sync" — and the whole point of the hardware test
//! is to get an unambiguous answer. `ps2_rt::gs::NTSC_NONINTERLACED` is
//! there for anyone whose console is NTSC.

#![no_std]
#![no_main]

use ps2_rt::gs;

use ps2_rt as _; // pulls in the panic handler and _start

const MODE: gs::VideoMode = gs::PAL_NONINTERLACED;
const SCREEN_WIDTH: u16 = 640;

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

const _: () = assert!(BGCOLOR_VALUE == 0xff);
const _: () = assert!(PMODE_VALUE == 0xa5);

/// Published once every register write below has happened.
const PROOF_OF_LIFE_VALUE: u32 = 0x5011_D000;

#[no_mangle]
extern "C" fn rust_main() -> ! {
    unsafe {
        gs::init_display(&MODE, SCREEN_WIDTH);
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
