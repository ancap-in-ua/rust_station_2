//! Entry point and bss initialization.
//!
//! The actual CPU entry point (`_start`) is a few lines of hand-written
//! MIPS assembly: it sets up `$gp` and `$sp` (there is no stack on entry)
//! and then jumps into `rt_start`, from which everything else is plain
//! Rust.

use core::arch::global_asm;

#[cfg(target_arch = "mips64")]
global_asm!(
    r#"
    .section .text.start, "ax", @progbits
    .global _start
    .set noreorder
_start:
    .set noat
    lui   $gp, %hi(_gp)
    addiu $gp, $gp, %lo(_gp)
    .set at

    lui   $sp, %hi(_stack_top)
    addiu $sp, $sp, %lo(_stack_top)

    jal   rt_start
    nop
.Lhang:
    b     .Lhang
    nop
    .set reorder
    "#
);

#[cfg(target_arch = "mips64")]
unsafe extern "C" {
    static mut __bss_start: u8;
    static mut __bss_end: u8;
}

/// Byte span between two addresses, or 0 if `end` doesn't come after
/// `start`. Split out of `zero_bss` so the arithmetic can be checked
/// independently of the real `__bss_start`/`__bss_end` symbols, which only
/// exist once linked against `link.ld`.
///
/// Checked with `const` assertions below rather than `#[cfg(test)]`: the
/// workspace's `.cargo/config.toml` sets `build-std = ["core"]` globally
/// (needed for the no-std PS2 target), and that's incompatible with
/// linking the host's prebuilt `std` that a `cargo test` harness needs —
/// confirmed by hand, not just assumed; see docs/progress.md. `const`
/// assertions run during every normal `cargo build`/`cargo check` instead,
/// no test harness required.
const fn bss_len(start: usize, end: usize) -> usize {
    end.saturating_sub(start)
}

const _: () = assert!(bss_len(0x1234, 0x1234) == 0);
const _: () = assert!(bss_len(0x0010_0000, 0x0010_0040) == 0x40);
// Should never happen given a correct linker script (bss_end is always >=
// bss_start), but zero_bss must not wrap around to a huge length and stomp
// memory if it somehow did.
const _: () = assert!(bss_len(0x0010_0040, 0x0010_0000) == 0);

#[cfg(target_arch = "mips64")]
unsafe fn zero_bss() {
    let start = core::ptr::addr_of_mut!(__bss_start);
    let end = core::ptr::addr_of!(__bss_end);
    let len = bss_len(start as usize, end as usize);
    unsafe {
        core::ptr::write_bytes(start, 0, len);
    }
}

// Binary crates must provide this: `#[no_mangle] extern "C" fn rust_main() -> !`.
#[cfg(target_arch = "mips64")]
unsafe extern "C" {
    fn rust_main() -> !;
}

#[cfg(target_arch = "mips64")]
#[no_mangle]
extern "C" fn rt_start() -> ! {
    unsafe {
        zero_bss();
        rust_main()
    }
}
