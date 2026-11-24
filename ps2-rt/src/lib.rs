#![no_std]
#![feature(asm_experimental_arch)]

mod boot;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
