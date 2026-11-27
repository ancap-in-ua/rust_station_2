//! next-steps.md #4: a tool that exercises `alloc`, now that
//! `ps2_rt::heap` provides a `#[global_allocator]` (see
//! docs/progress.md). Proof-of-life in the same style as `hello`: build
//! something on the heap, derive a value from its *contents* (not a
//! hardcoded constant -- so a broken allocator that returns garbage or
//! aliased memory would plausibly change this value), and write it to a
//! fixed RAM address.
//!
//! Dynamically verified as of docs/progress.md 2026-11-27: `just
//! test-qemu` boots this and confirms the allocator handed out usable,
//! non-aliasing memory (the expected `0xA110008C` lands at `0x1000`, and
//! the heap at halt holds the dead 4-element Vec followed by the live
//! 8-element one). On a generic MIPS64 CPU model, though, not a PS2 --
//! see scripts/lib-qemu.sh for what that does and doesn't settle.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;

use ps2_rt as _; // pulls in the panic handler, allocator, and _start

#[no_mangle]
extern "C" fn rust_main() -> ! {
    let mut v: Vec<u32> = Vec::new();
    for i in 0..8u32 {
        v.push(i * i);
    }
    let sum: u32 = v.iter().sum(); // 0+1+4+9+16+25+36+49 = 140 = 0x8c

    ps2_rt::set_proof(0xA110_0000 | sum);
    ps2_rt::ps2_rt_halt()
}
