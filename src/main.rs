#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

use core::panic::PanicInfo;
use interrupts::init_idt;

mod serial;
mod interrupts;
mod lib;

// This function named panic is our panic handler
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop{}
}

// We dont wan't the compiler to generate some 
// weird nebulous string for our name so we 
// put no mangle
#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> ! {
    serial_println!("Hello from Rust!");

    init();

    serial_println!("IDT initialized!");


    loop{}
}

