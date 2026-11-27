//! Minimal proof-of-life: no console, no display output yet (see README) —
//! just write a known value to a fixed, otherwise-unused RAM address and
//! spin. `just test-qemu` asserts it landed; by hand, check PCSX2's
//! memory viewer (Debug > Memory) at address 0x00001000, or a GDB
//! `x/1xw 0x1000` over its debug stub.

#![no_std]
#![no_main]

use ps2_rt as _; // pulls in the panic handler and _start

const PROOF_OF_LIFE_ADDR: usize = 0x0000_1000;
const PROOF_OF_LIFE_VALUE: u32 = 0xC0FF_EE42;

#[no_mangle]
extern "C" fn rust_main() -> ! {
    unsafe {
        core::ptr::write_volatile(PROOF_OF_LIFE_ADDR as *mut u32, PROOF_OF_LIFE_VALUE);
    }
    ps2_rt::ps2_rt_halt()
}
