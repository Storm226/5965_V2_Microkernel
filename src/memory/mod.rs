// Declare your allocator here

//#[global_allocator]
//pub static ALLOCATOR: YourAllocatorType = ...;

use crate::_bootinfo;
use crate::MemoryMapTag;
use crate::kernel_end;
use crate::multibootv2;
use crate::multibootv2::MemoryArea;
use crate::round_up;
use crate::serial_println;
use core::ptr;
use core::slice;

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

    // tracks 4k pages in 2mb superpage
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
    let page_array: &[Page_Array_Element] = unsafe {
        slice::from_raw_parts(
            kernel_end() as *const Page_Array_Element,
            four_k_page_count as usize,
        )
    };

    // we must ensure that we do not give away ptrs to memory and overwrite our page_array
    let page_array_size_bytes = four_k_page_count as usize * core::mem::size_of::<Page_Array_Element>();
    let page_array_pages = (page_array_size_bytes + 4095) / 4096; // round up

    serial_println!("the number of pages our page_array occupies is: {}", page_array_pages);
    
    // we need to figure out how many 2mb pages we can get and we also need to define the boundary of where we begin
    // allocating 2mb pages
    two_mb_page_count = four_k_page_count / 512;

    //8556544 (decimal) = 0x0082A000 (hex)
    // kernel end should be rounded up to some 4kb page boundary, so, i suppose
    let pages_up_to_kernel_end: u64 = kernel_end() / 4096;

    // YES IT MAKES SENSE
    // 2089! thats how many pages we mark as unavail

    serial_println!("The kernel ends at : {}", pages_up_to_kernel_end);

    // how many 4k pages are useful to the system
    // this number indicates starting from the next 4k page boundary from kernel end, how
    // many useful_pages there are for our system
    //let mut useful_pages_count: u64 = four_k_page_count - pages_up_to_kernel_end;
    let mut useful_pages_count: u64 = four_k_page_count - pages_up_to_kernel_end - (page_array_pages as u64);

    let mut not_useful_pages_count: u64 = four_k_page_count - useful_pages_count;

    let mut remainder: u64 = useful_pages_count % 512;

    // 32639 total 4k pages (on my machine) -> 383 first pages can never be a complete 2mb page
    serial_println!("The total number of four kb pages is:  {}", four_k_page_count);

    // 63 total 2mb pages (on my machine)
    serial_println!("The total number of 2MB pages is:  {}", two_mb_page_count);

    // 30550
    serial_println!("The number of useful four kb pages is:  {}", useful_pages_count);

    // 2089
    serial_println!(
        "The amount of fourkb pages which come first and so are not useful is: {}",
        not_useful_pages_count
    );

    // 342
    serial_println!(
        "The number of 4kb pages which are useful but cannot become a 2mb page is:  {}",
        remainder
    );

    unsafe {
        // Use usize for indices
        let len_usize: usize = page_array.len();
        let not_useful_usize: usize = not_useful_pages_count as usize;
        let useful_usize: usize = len_usize - not_useful_usize;
        let remainder_usize: usize = remainder as usize;

        // Basic sanity check
        debug_assert!(page_array_pages as u64 + pages_up_to_kernel_end <= four_k_page_count);

        // 1) initialize all fields to safe defaults for entire array (no garbage)
        for idx in 0..len_usize {
            let p = page_array.as_ptr().add(idx) as *mut Page_Array_Element;
            (*p).next_4k = ptr::null_mut();
            (*p).prev_4k = ptr::null_mut();
            (*p).next_2mb = ptr::null_mut();
            (*p).prev_2mb = ptr::null_mut();
            (*p).state = State::Unavail;
            (*p).count = 0;
        }

        // 2) mark pages before first usable page as Unavail and set 4k links there
        for idx in 0..not_useful_usize {
            let p = page_array.as_ptr().add(idx) as *mut Page_Array_Element;
            (*p).state = State::Unavail;
            (*p).prev_4k = if idx == 0 { ptr::null_mut() } else { page_array.as_ptr().add(idx - 1) as *mut _ };
            (*p).next_4k = if idx + 1 >= len_usize { ptr::null_mut() } else { page_array.as_ptr().add(idx + 1) as *mut _ };
        }

        // 3) Now the useful region: consume remainder (Free4K), then many 512-page superpages, then tail Free4K
        let mut idx = not_useful_usize;

        // consume remainder -> those useful pages that can't be part of a 2MB page
        let mut rem = remainder_usize;
        while rem > 0 && idx < len_usize {
            let p = page_array.as_ptr().add(idx) as *mut Page_Array_Element;
            (*p).state = State::Free4K;
            (*p).prev_4k = if idx == 0 { ptr::null_mut() } else { page_array.as_ptr().add(idx - 1) as *mut _ };
            (*p).next_4k = if idx + 1 >= len_usize { ptr::null_mut() } else { page_array.as_ptr().add(idx + 1) as *mut _ };
            idx += 1;
            rem -= 1;
        }

        // carve full 2MB superpages (512 * 4KB)
        while idx + 512 <= len_usize {
            // head of superpage
            let head = page_array.as_ptr().add(idx) as *mut Page_Array_Element;

            // set prev_2mb for head (null if this is the first)
            if counter_2mb_pages_init == 0 {
                (*head).prev_2mb = ptr::null_mut();
            } else {
                // previous head is at idx - 512
                (*head).prev_2mb = page_array.as_ptr().add(idx - 512) as *mut Page_Array_Element;
                // also link previous head's next_2mb to this head
                let prev_head = page_array.as_ptr().add(idx - 512) as *mut Page_Array_Element;
                (*prev_head).next_2mb = head;
            }

            // set head metadata
            (*head).state = State::Free2MB;
            (*head).count = 512;

            // initialize all 512 pages in the superpage
            for j in 0..512 {
                let pidx = idx + j;
                let p = page_array.as_ptr().add(pidx) as *mut Page_Array_Element;
                (*p).state = State::Free2MB; // mark as part of a 2MB block
                (*p).count = if j == 0 { 512 } else { 0 }; // only head stores count
                (*p).prev_4k = if pidx == 0 { ptr::null_mut() } else { page_array.as_ptr().add(pidx - 1) as *mut _ };
                (*p).next_4k = if pidx + 1 >= len_usize { ptr::null_mut() } else { page_array.as_ptr().add(pidx + 1) as *mut _ };
                // prev_2mb/next_2mb remain null for non-heads (head was already set)
            }

            counter_2mb_pages_init += 1;
            idx += 512;
        }

        // leftover tail pages (less than 512) -> Free4K
        while idx < len_usize {
            let p = page_array.as_ptr().add(idx) as *mut Page_Array_Element;
            (*p).state = State::Free4K;
            (*p).prev_4k = if idx == 0 { ptr::null_mut() } else { page_array.as_ptr().add(idx - 1) as *mut _ };
            (*p).next_4k = if idx + 1 >= len_usize { ptr::null_mut() } else { page_array.as_ptr().add(idx + 1) as *mut _ };
            idx += 1;
        }
    } // end unsafe

}

// once the page array is actually set up properly
// we need a a pointer to head of 4k list,
// and a pointer to head of 2mb list

// after that, we just need to implement merge and split
// and we need to implement when they call free

// okay so given a range of memory, (start, end)
// we just wanna say how many pages of memory are there
fn count_4k_pages(area: &MemoryArea, page_count: &mut u64) {
    *page_count += (area.size() / 4096) as u64;
}

//
//give_me_a_page(int n_pages){}

// given some 4kb pages, merge them into a contiguous
// 2MB page
//merge(){}

// given a 2MB page, split into a bunch
// of 4kb pages
//split(){}
