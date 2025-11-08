// Declare your allocator here

// #[global_allocator]
// pub static ALLOCATOR: YourAllocatorType = ...;

use crate::_bootinfo;
use crate::MemoryMapTag;
use crate::multibootv2;
use crate::kernel_end;
use crate::serial_println;
use crate::round_up;
use crate::multibootv2::MemoryArea;

// setup
pub fn init_alloc() {
    // lets figure out how many 4kb pages there are total first
    // grub hands us this data structure which is ignorant of the kernel,
    // so we should probably think about that fact as we reason about avail
    // memory
    let mut total_area_bytes: u64 = 0;
    let mut four_k_page_count: u32 = 0;

    let bootinfo = unsafe {
        serial_println!("multibootv2 tag found at {:x}", _bootinfo as usize);
        multibootv2::load(_bootinfo)
    };
    // we do indeed have kernel end address
    // kernel end: 8560640
    let memory_map_tag: &MemoryMapTag = bootinfo.memory_map_tag().unwrap();
    // Iterate over the memory areas
    for area in memory_map_tag.memory_areas() {
        serial_println!(
            "Memory area: start=0x{:x}, end=0x{:x}, size={} bytes, type={}",
            area.start_address(),
            area.end_address(),
            area.size(),
            area.typ()
        );

        // we have a mut ref to our page count
        // for each area accumulate n_pages as apt
        account_pages(area, &mut four_k_page_count);
                    
    }

    serial_println!("the total area size is {}", total_area_bytes);
    serial_println!("the total 4k_page count is {}", four_k_page_count);
}


// okay so given a range of memory, (start, end)
// we want to make page table entries for the kernel 
// splitting it up into 4kb pages (for now)
fn account_pages(area: &MemoryArea,  page_count : &mut u32){

    if(area.start_address() == 0x100000){
        serial_println!("caught");
    }
    
    let x = kernel_end();
    let y = area.end_address();
    let size = y - x;
    serial_println!("size is {}", size);
    serial_println!("area size is {}", area.size());


    

    let n = area.size() / 4096;
    *page_count += n as u32;
}


//
//give_me_a_page(int n_pages){}

// given some 4kb pages, merge them into a contiguous
// 2MB page
//merge(){}

// given a 2MB page, split into a bunch
// of 4kb pages
//split(){}
