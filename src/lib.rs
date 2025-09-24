#![no_std]
#![feature(abi_x86_interrupt)] // needed for x86-interrupt handlers

pub mod serial;      // optional, for printing
pub mod interrupts;  // contains your IDT initialization

/// Initialize the kernel subsystems
pub fn init() {
    interrupts::init_idt();
}
