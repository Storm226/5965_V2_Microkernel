#![cfg_attr(not(test), no_std, no_main)]
#![allow(static_mut_refs)]
#![allow(unused)]

mod architecture;
mod cpu;
mod error;
mod gdt;
mod interrupt;
mod memory;
mod multibootv2;
mod serial;

use crate::multibootv2::BootInformation;
use crate::multibootv2::MemoryMapTag;
use core::panic::PanicInfo;

// A: we may not need this
use crate::architecture::kernel_end;

use crate::gdt::init_cpu;

unsafe extern "C" {
    #[unsafe(no_mangle)]
    static _bootinfo: usize;
}

// This function named panic is our panic handler
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
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
    }

    unsafe {
        interrupt::init();
    }

    unsafe {
        interrupt::init_cpu();
    }

    // okay so interrupts are enabled
    // now we can begin setting up allocator

    memory::init_alloc();

    loop {}
}

//~
// Simple helper function to print out boot information
//
fn print_multiboot_information(bootinfo: BootInformation) {
    // print some basic information around boot
    serial_println!("Boot info start address: {:#x}", bootinfo.start_address());
    serial_println!("Boot info end address: {:#x}", bootinfo.end_address());
    serial_println!("Boot info total size: {} bytes", bootinfo.total_size());
}
