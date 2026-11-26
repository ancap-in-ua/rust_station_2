//! next-steps.md #4: a tool that exercises `alloc`, now that
//! `ps2_rt::heap` provides a `#[global_allocator]` (see
//! docs/progress.md). Proof-of-life in the same style as `hello`: build
//! something on the heap, derive a value from its *contents* (not a
//! hardcoded constant -- so a broken allocator that returns garbage or
//! aliased memory would plausibly change this value), and write it to a
//! fixed RAM address.
//!
//! NOT dynamically verified -- same caveat as every other tool so far,
//! no BIOS dump available in this environment (see docs/next-steps.md
//! #1).

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;

use ps2_rt as _; // pulls in the panic handler, allocator, and _start

const PROOF_OF_LIFE_ADDR: usize = 0x0000_1000;

#[no_mangle]
extern "C" fn rust_main() -> ! {
    let mut v: Vec<u32> = Vec::new();
    for i in 0..8u32 {
        v.push(i * i);
    }
    let sum: u32 = v.iter().sum(); // 0+1+4+9+16+25+36+49 = 140 = 0x8c

    unsafe {
        core::ptr::write_volatile(PROOF_OF_LIFE_ADDR as *mut u32, 0xA110_0000 | sum);
    }
    loop {}
}
