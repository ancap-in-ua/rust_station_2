//! Entry point and bss initialization.
//!
//! The actual CPU entry point (`_start`) is a few lines of hand-written
//! MIPS assembly: it sets up `$gp` and `$sp` (there is no stack on entry)
//! and then jumps into `rt_start`, from which everything else is plain
//! Rust.

use core::arch::global_asm;

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

unsafe extern "C" {
    static mut __bss_start: u8;
    static mut __bss_end: u8;
}

unsafe fn zero_bss() {
    let start = core::ptr::addr_of_mut!(__bss_start);
    let end = core::ptr::addr_of!(__bss_end);
    let len = (end as usize).saturating_sub(start as usize);
    unsafe {
        core::ptr::write_bytes(start, 0, len);
    }
}

// Binary crates must provide this: `#[no_mangle] extern "C" fn rust_main() -> !`.
unsafe extern "C" {
    fn rust_main() -> !;
}

#[no_mangle]
extern "C" fn rt_start() -> ! {
    unsafe {
        zero_bss();
        rust_main()
    }
}
