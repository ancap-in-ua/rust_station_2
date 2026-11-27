//! Cycles the screen red -> green -> blue forever, by redrawing the
//! framebuffer each time.
//!
//! This exists for one reason: a *static* screen is an ambiguous test
//! result. If a solid fill shows black, that could mean the drawing
//! path doesn't work, or that the ELF never booted, or that the TV
//! didn't sync — and with no console output there's nothing to tell
//! them apart.
//!
//! A screen that *changes* is unambiguous. Motion means the ELF booted,
//! the BIOS syscall returned, the PCRTC is live, DMA to the GIF works,
//! and the GS is drawing. Nothing means the problem is upstream of all
//! of that.
//!
//! So this is the tool to run first on a console, and the one to reach
//! for when something stops working.
//!
//! Originally this drove `BGCOLOR` through the PCRTC merge circuit,
//! which PCSX2 cannot render (see `tools/solid-color`). It now draws
//! real pixels via `ps2_rt::draw`, so it works under emulation too.
//!
//! Not checked by `just test-qemu`: it never reaches `ps2_rt_halt` (it
//! loops forever by design), and under qemu it would hang in the BIOS
//! syscall anyway. `just test-static` and `just test-pcsx2` both cover
//! it — the proof-of-life value is published *before* the loop starts,
//! so PINE can still read it.

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

const COLORS: [(u8, u8, u8); 3] = [(0xff, 0x00, 0x00), (0x00, 0xff, 0x00), (0x00, 0x00, 0xff)];

/// Roughly half a second on a ~294 MHz EE. Deliberately a busy loop
/// rather than a timer or a VSync poll: this tool's whole job is to be
/// obviously-working or obviously-not, and a spin loop has nothing in
/// it that can itself be the thing that's broken.
const DELAY_ITERATIONS: u32 = 30_000_000;

/// Published before the loop starts, so `just test-pcsx2` can confirm
/// the setup ran even though this tool never halts.
const PROOF_OF_LIFE_VALUE: u32 = 0xC0C0_0000;

/// In `.bss` so the DMAC can read it out of RAM.
static mut PACKET: [Qword; draw::FILL_PACKET_QWORDS] =
    [Qword::ZERO; draw::FILL_PACKET_QWORDS];

fn spin(iterations: u32) {
    for _ in 0..iterations {
        core::hint::black_box(iterations);
    }
}

#[no_mangle]
extern "C" fn rust_main() -> ! {
    let packet = unsafe {
        gs::init_display(&MODE, SCREEN_WIDTH);
        gs::write_reg(gs::REG_PMODE, draw::pmode_show_circuit1());
        &mut *core::ptr::addr_of_mut!(PACKET)
    };
    ps2_rt::set_proof(PROOF_OF_LIFE_VALUE);

    // The index advances with a compare-and-reset rather than the
    // obvious `idx = (idx + 1) % COLORS.len()`. That is not a style
    // choice: with the modulo, this tool reproducibly fails to reach
    // `rust_main` *at all* on PCSX2 -- the proof-of-life value is never
    // published, even with `set_proof` moved to the first statement of
    // the function. The identical ELF runs correctly under qemu, so the
    // binary is sound and the fault is somewhere in PCSX2. Cause
    // unknown; the evidence is in docs/progress.md 2026-11-27. Avoiding
    // the division sidesteps it. If you reintroduce `%` here, expect
    // `just test-pcsx2` to start failing for reasons unrelated to
    // whatever you were changing.
    let mut idx = 0usize;
    loop {
        unsafe {
            draw::fill_screen(packet, SCREEN_WIDTH, SCREEN_HEIGHT, COLORS[idx]);
        }
        spin(DELAY_ITERATIONS);
        idx += 1;
        if idx == COLORS.len() {
            idx = 0;
        }
    }
}
