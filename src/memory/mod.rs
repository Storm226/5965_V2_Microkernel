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
use core::ptr;

#[repr(C)]
pub struct State {
    // Define fields of State as needed
    // Example:
    pub unavail, 
    pub free_4k
    pub alloc_4k,
    pub free_2mb,
    pub alloc_2mb,
}

#[repr(C)]
pub struct Page_Array_Element {
    // Pointers for 4 KB page linked list
    pub next_4k: *mut Page_Array_Element,
    pub prev_4k: *mut Page_Array_Element,

    // Pointers for 2 MB page linked list
    pub next_2mb: *mut Page_Array_Element,
    pub prev_2mb: *mut Page_Array_Element,

    // Embedded state
    pub state: State,

    // Count of something (e.g., number of pages)
    pub count: i32,
}



// setup
pub fn init_alloc() {
    // lets figure out how many 4kb pages there are total first
    // grub hands us this data structure which is ignorant of the kernel,
    // so we should probably think about that fact as we reason about avail
    // memory
    let mut total_area_bytes: u64 = 0;
    let mut four_k_page_count: u32 = 0;
    let mut two_mb_page_count: u32 = 0;
    
    

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
    }
    // okay so here is our page_array
    let page_array: &[Page_Array_Element] = unsafe {slice::from_raw_parts(kernel_end() as *const Page_Array_Element, four_k_page_count as usize)};


    // we need to figure out how many 2mb pages we can get and we also need to define the boundary of where we begin
    // allocating 2mb pages
    two_mb_page_count = four_k_page_count / 512; 

    
    unsafe {
    // We’re assuming `page_array` points to valid, writable memory
            let len = page_array.len();
        
            for i in 0..len 
                {
                    let elem_ptr = page_array.as_ptr().add(i) as *mut Page_Array_Element;
            
                    // Set previous pointer
                    if i == 0 {
                        (*elem_ptr).prev_4k = ptr::null_mut();
                    } else {
                        (*elem_ptr).prev_4k = page_array.as_ptr().add(i - 1) as *mut Page_Array_Element;
                    }
            
                    // Set next pointer
                    if i + 1 == len {
                        (*elem_ptr).next_4k = ptr::null_mut();
                    } else {
                        (*elem_ptr).next_4k = page_array.as_ptr().add(i + 1) as *mut Page_Array_Element;
                    }
                }
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
