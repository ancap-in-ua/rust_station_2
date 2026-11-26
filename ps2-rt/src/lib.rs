#![no_std]
#![feature(asm_experimental_arch)]

mod boot;
pub mod gs;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
