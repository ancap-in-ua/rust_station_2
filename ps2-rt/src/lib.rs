#![no_std]
#![feature(asm_experimental_arch)]

extern crate alloc;

mod boot;
pub mod gs;
pub mod heap;
pub mod syscall;

use core::panic::PanicInfo;

/// Tag values identifying which terminal state a spinning tool is in.
/// Passed to [`spin_forever`] in `$a0`, so they're readable from a
/// debugger even with no symbols loaded.
pub const TAG_HALT: u32 = 0x4F4B_4F4B; // "OKOK"
pub const TAG_PANIC: u32 = 0x5041_4E21; // "PAN!"

/// The actual spin. Takes a tag it never uses for anything, kept alive
/// across the loop by `black_box`.
///
/// The tag is load-bearing in two ways that aren't obvious. It's what a
/// debugger reads to tell the two terminal states apart without symbols
/// — and, less obviously, it's what keeps them *apart at all*: an
/// `#[inline(never)]` wrapper whose whole body is `loop {}` compiles to
/// a byte-identical function in both cases, and LLVM/lld happily fold
/// the two into one address (observed: `ps2_rt_halt` and
/// `ps2_rt_panicked` came out at the same address before this, making
/// the distinction useless). Differing in one immediate is enough to
/// stop that.
#[inline(never)]
fn spin_forever(tag: u32) -> ! {
    loop {
        core::hint::black_box(tag);
    }
}

/// A tool's terminal state: there is no OS to exit to, so "done" means
/// spin forever.
///
/// Deliberately a named, `#[no_mangle]`, never-inlined function rather
/// than an open-coded `loop {}` in each tool: it gives a debugger a
/// stable symbol to break on for "the program finished", which is what
/// `scripts/verify-qemu.sh` uses to know when it's safe to inspect
/// memory. A bare `loop {}` compiles to an anonymous backwards branch at
/// whatever address LLVM happens to put it, so a test would have to
/// hardcode an address that moves on every unrelated code change.
#[inline(never)]
#[no_mangle]
pub extern "C" fn ps2_rt_halt() -> ! {
    spin_forever(TAG_HALT)
}

/// Where a panic ends up. Separate from [`ps2_rt_halt`] purely so the two
/// are distinguishable: both just spin, but a debugger stopped here means
/// "panicked", not "finished", and `scripts/verify-qemu.sh` reports that
/// difference instead of leaving a panic looking like a tool that
/// mysteriously never wrote its result.
///
/// May be garbage-collected out of tools with no reachable panic path
/// (`hello` has none), so anything breaking on it must treat the symbol
/// as optional.
#[inline(never)]
#[no_mangle]
pub extern "C" fn ps2_rt_panicked() -> ! {
    spin_forever(TAG_PANIC)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    ps2_rt_panicked()
}
