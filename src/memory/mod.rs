// Declare your allocator here

//#[global_allocator]
//pub static ALLOCATOR: YourAllocatorType = ...;

use crate::_bootinfo;
use crate::MemoryMapTag;
use crate::multibootv2;
use crate::kernel_end;
use crate::serial_println;
use crate::round_up;
use crate::multibootv2::MemoryArea;
use core::slice;


pub struct Page_Arrays{
    pages_4k : &'static mut [Page4k]
}

pub struct Page4k{
    state: Page_State,
    prev: u64,
    next: u64
}

pub struct Page2MB{
    state: Page_State,
    prev: u64,
    next: u64
}

pub enum Page_State {
    Unavailable, 
    Alloc,
    Free
}



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

        
        // for each area accumulate n_pages as apt
    count_4k_pages(area, &mut four_k_page_count);
    
    // okay so here is our 4k page array
    let array_4k: &[Page4k] = unsafe {slice::from_raw_parts(kernel_end() as *const Page4k, four_k_page_count as usize)};

    Page_Arrays{array_4k};    

    }
}


// okay so given a range of memory, (start, end)
// we just wanna say how many pages of memory are there
fn count_4k_pages(area: &MemoryArea,  page_count : &mut u32){ *page_count += (area.size() / 4096) as u32;}


//
//give_me_a_page(int n_pages){}

// given some 4kb pages, merge them into a contiguous
// 2MB page
//merge(){}

// given a 2MB page, split into a bunch
// of 4kb pages
//split(){}
