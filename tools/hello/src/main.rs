//! Minimal proof-of-life: no console, no display output yet (see README) —
//! just publish a known value through [`ps2_rt::set_proof`] and spin.
//! `just test-qemu` and `just test-pcsx2` both assert it landed; by hand,
//! look up the `PS2_RT_PROOF` symbol with `nm` and read that address.

#![no_std]
#![no_main]

use ps2_rt as _; // pulls in the panic handler and _start

const PROOF_OF_LIFE_VALUE: u32 = 0xC0FF_EE42;

#[no_mangle]
extern "C" fn rust_main() -> ! {
    ps2_rt::set_proof(PROOF_OF_LIFE_VALUE);
    ps2_rt::ps2_rt_halt()
}
