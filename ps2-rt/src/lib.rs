#![no_std]
#![feature(asm_experimental_arch)]

extern crate alloc;

mod boot;
pub mod gs;
pub mod heap;
pub mod syscall;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
