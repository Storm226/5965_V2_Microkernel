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
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum State {
    Unavail,
    Free4K,
    Alloc4K,
    Free2MB,
    Alloc2MB,
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
    let mut four_k_page_count: u64 = 0;
    let mut two_mb_page_count: u64 = 0;
    
    

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


    //8556544 (decimal) = 0x0082A000 (hex)
    // kernel end should be rounded up to some 4kb page boundary, so, i suppose
    let pages_up_to_kernel_end : u64 = kernel_end() / 4096;

    // YES IT MAKES SENSE 
    // 2089! thats how many pages we mark as unavail
    
    serial_println!("kernel end is : {}", pages_up_to_kernel_end);
    
    // how many 4k pages are useful to the system
    // this number indicates starting from the next 4k page boundary from kernel end, how 
    // many useful_pages there are for our system
    let mut useful_pages_count : u64 = four_k_page_count - pages_up_to_kernel_end;

    // 5 not useful , 10 useful
    let mut not_useful_pages_count : u64 = four_k_page_count - useful_pages_count;

    let mut remainder : u64 = useful_pages_count % 512;

    // 32639 total 4k pages (on my machine) -> 383 first pages can never be a complete 2mb page
    serial_println!("four kb page count {}", four_k_page_count);

    // 63 total 2mb pages (on my machine)
    serial_println!("two mb page count {}", two_mb_page_count);

    serial_println!("useful four kb page count {}", useful_pages_count);

    serial_println!("the first n not useful four kb page count {}", not_useful_pages_count);

    serial_println!("the first n useful pages which cant be a 2mb page  {}", remainder);

    
    unsafe {
    // We’re assuming `page_array` points to valid, writable memory
            let len = page_array.len();
        
            for i in 0..len 
                {
                    let element_ptr = page_array.as_ptr().add(i) as *mut Page_Array_Element;
            

                    if i < not_useful_pages_count.try_into().unwrap(){
                        (*element_ptr).state = State::Unavail;

                    }
                    else {
                        (*element_ptr).state  = State::Free4K;
                    }


                    // Set previous pointer
                    if i == 0 {
                        (*element_ptr).prev_4k = ptr::null_mut();
                    } else {
                        (*element_ptr).prev_4k = page_array.as_ptr().add(i - 1) as *mut Page_Array_Element;
                    }
            
                    // Set next pointer
                    if i + 1 == len {
                        (*element_ptr).next_4k = ptr::null_mut();
                    } else {
                        (*element_ptr).next_4k = page_array.as_ptr().add(i + 1) as *mut Page_Array_Element;
                    }

                    


                }
          }
}






// okay so given a range of memory, (start, end)
// we just wanna say how many pages of memory are there
fn count_4k_pages(area: &MemoryArea,  page_count : &mut u64){ *page_count += (area.size() / 4096) as u64;}


//
//give_me_a_page(int n_pages){}

// given some 4kb pages, merge them into a contiguous
// 2MB page
//merge(){}

// given a 2MB page, split into a bunch
// of 4kb pages
//split(){}
