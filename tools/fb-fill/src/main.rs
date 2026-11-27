//! Fills the framebuffer with a solid colour by actually *drawing* --
//! a full-screen sprite pushed to the GS through a GIF packet over DMA.
//!
//! This is the approach `tools/solid-color` was trying to avoid, and the
//! first thing in this project to put a visible image on screen. It's
//! here because avoiding it turned out to be a dead end under emulation:
//! PCSX2's merge circuit returns without presenting a frame when no read
//! circuit has a framebuffer texture, so a display whose VRAM was never
//! written shows nothing no matter how correct its PCRTC registers are
//! (docs/progress.md 2026-11-27). Putting real pixels in VRAM is the
//! only way to get a visible result in PCSX2 -- and it's what every
//! actual PS2 program does, so it had to exist regardless.
//!
//! The drawing side of the GS is reached very differently from the
//! display side: `PRIM`/`RGBAQ`/`XYZ2`/`FRAME_1`/... are not
//! memory-mapped registers but indices inside a GIF packet. See
//! `ps2_rt::gif` and `ps2_rt::draw`.

#![no_std]
#![no_main]

use ps2_rt::draw;
use ps2_rt::gif::Qword;
use ps2_rt::gs;

use ps2_rt as _; // pulls in the panic handler and _start

/// PAL to match the only console available -- see `tools/solid-color`.
const MODE: gs::VideoMode = gs::PAL_NONINTERLACED;
const SCREEN_WIDTH: u16 = 640;
const SCREEN_HEIGHT: u16 = MODE.dh;

const FILL: (u8, u8, u8) = (0xff, 0x00, 0x00);

/// Published once the packet has been sent and the DMA has drained.
const PROOF_OF_LIFE_VALUE: u32 = 0xFBF1_1100;

/// In `.bss` so the DMAC can read it out of RAM; `Qword` carries the
/// 16-byte alignment DMA requires.
static mut PACKET: [Qword; draw::FILL_PACKET_QWORDS] =
    [Qword::ZERO; draw::FILL_PACKET_QWORDS];

#[no_mangle]
extern "C" fn rust_main() -> ! {
    unsafe {
        gs::init_display(&MODE, SCREEN_WIDTH);
        gs::write_reg(gs::REG_PMODE, draw::pmode_show_circuit1());

        let packet = &mut *core::ptr::addr_of_mut!(PACKET);
        draw::fill_screen(packet, SCREEN_WIDTH, SCREEN_HEIGHT, FILL);
    }

    ps2_rt::set_proof(PROOF_OF_LIFE_VALUE);
    ps2_rt::ps2_rt_halt()
}
