//! `solid-color`, but cycling red -> green -> blue forever.
//!
//! This exists for one reason: a *static* screen is an ambiguous test
//! result on real hardware. If `solid-color` shows black, that could
//! mean the PMODE/BGCOLOR approach doesn't work, or that the ELF never
//! booted, or that the TV didn't sync — and with no console output
//! there's nothing to tell them apart.
//!
//! A screen that *changes* is unambiguous. Motion means the ELF booted,
//! the BIOS syscall returned, the PCRTC is live, and BGCOLOR reaches
//! the display. Nothing means something upstream of all that is wrong,
//! and the next step is to find out which — not to re-litigate the
//! blend equation.
//!
//! So this is the tool to run first on a console, before `solid-color`.
//!
//! Not checked by `just test-qemu`: it never reaches `ps2_rt_halt` (it
//! loops forever by design), and under qemu it would hang in the BIOS
//! syscall anyway, exactly as `solid-color` does. `just test-static`
//! and `just test-pcsx2` both cover it — the proof-of-life value is
//! published *before* the loop starts, so PINE can still read it.

#![no_std]
#![no_main]

use ps2_rt::gs;

use ps2_rt as _; // pulls in the panic handler and _start

/// PAL to match the only console this project can test on -- see
/// `tools/solid-color`'s module doc.
const MODE: gs::VideoMode = gs::PAL_NONINTERLACED;
const SCREEN_WIDTH: u16 = 640;

/// Same merge-circuit configuration as `solid-color`: take 100% of the
/// output from BGCOLOR.
const PMODE_VALUE: u64 = gs::pmode(true, false, true, false, true, 0);
const _: () = assert!(PMODE_VALUE == 0xa5);

const COLORS: [(u8, u8, u8); 3] = [(0xff, 0x00, 0x00), (0x00, 0xff, 0x00), (0x00, 0x00, 0xff)];

/// Roughly half a second on a ~294 MHz EE. Deliberately a busy loop
/// rather than a timer or a VSync poll: this tool's whole job is to be
/// obviously-working or obviously-not, and a spin loop has nothing in
/// it that can itself be the thing that's broken.
const DELAY_ITERATIONS: u32 = 30_000_000;

fn spin(iterations: u32) {
    for _ in 0..iterations {
        core::hint::black_box(iterations);
    }
}

/// Published before the loop starts, so `just test-pcsx2` can confirm
/// the setup ran even though this tool never halts.
const PROOF_OF_LIFE_VALUE: u32 = 0xC0C0_0000;

#[no_mangle]
extern "C" fn rust_main() -> ! {
    unsafe {
        gs::init_display(&MODE, SCREEN_WIDTH);
        gs::write_reg(gs::REG_PMODE, PMODE_VALUE);
    }
    ps2_rt::set_proof(PROOF_OF_LIFE_VALUE);

    // The index advances with a compare-and-reset rather than the
    // obvious `idx = (idx + 1) % COLORS.len()`. That is not a style
    // choice: with the modulo, this tool reproducibly fails to reach
    // `rust_main` *at all* on PCSX2 -- the proof-of-life value is never
    // published, even with `set_proof` moved to the first statement of
    // the function. The identical ELF runs correctly under qemu
    // (reaches rust_main, publishes the proof, reaches set_gs_crt with
    // the right arguments), so the binary is sound and the fault is
    // somewhere in PCSX2. Cause unknown; the evidence is written up in
    // docs/progress.md 2026-11-27. Avoiding the division sidesteps it.
    // If you reintroduce `%` here, expect `just test-pcsx2` to start
    // failing for reasons unrelated to whatever you were changing.
    let mut idx = 0usize;
    loop {
        let (r, g, b) = COLORS[idx];
        unsafe {
            gs::write_reg(gs::REG_BGCOLOR, gs::bgcolor(r, g, b));
        }
        spin(DELAY_ITERATIONS);
        idx += 1;
        if idx == COLORS.len() {
            idx = 0;
        }
    }
}
