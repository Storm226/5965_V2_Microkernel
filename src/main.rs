#![cfg_attr(not(test), no_std, no_main)]
#![allow(static_mut_refs)]
#![allow(unused)]

mod cpu;
mod error;
mod gdt;
mod interrupt;
mod serial;

use core::panic::PanicInfo;

use crate::gdt::init_cpu;

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
   // serial_println!("Hello from Rust!");

    // i promise i will be safe and only
    // do this once per cpu reset
    unsafe {
        gdt::init_cpu();
        interrupt::init();
        interrupt::init_cpu();        
        };
    loop{}
}