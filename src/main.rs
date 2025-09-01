#![no_std]
#![no_main]

use core::panic::PanicInfo;

static HELLO: &[u8] = b"Hello World!";

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
    //println!("Hello from Rust!");
    let vga_buffer = 0xb8000 as *mut u8;

    for(i, &byte) in HELLO.iter().enumerate(){
        unsafe{
            *vga_buffer.offset(i as isize *2) = byte;
            *vga_buffer.offset(i as isize *2 +1) = 0xb;
        }
    }

    loop {}
}

