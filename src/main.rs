#![cfg_attr(not(test), no_std, no_main)]
#![allow(static_mut_refs)]

mod cpu;
mod error;
mod gdt;
mod interrupt;
mod serial;



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

    loop{}
}