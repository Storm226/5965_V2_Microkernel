// Declare your allocator here


pub mod test;

use crate::_bootinfo;
use crate::MemoryMapTag;
use crate::kernel_end;
use crate::multibootv2;
use crate::multibootv2::MemoryArea;
use crate::round_up;
use crate::serial_println;
use core::ptr;
use core::ptr::null;
use core::ptr::null_mut;
use core::slice;
use core::debug_assert;
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};
use core::option::Option;
use core::option::Option::Some;
use core::prelude::rust_2024::derive;
use core::prelude::rust_2024::global_allocator;



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
pub struct PageArrayElement {
    // Pointers for 4 KB page linked list
    pub next_4k: *mut PageArrayElement,
    pub prev_4k: *mut PageArrayElement,

    // Pointers for 2 MB page linked list
    pub next_2mb: *mut PageArrayElement,

    // we are gonna make a decision that every 4kb page
    // which is owned by a 2mb super page will have 
    // this value set to indicate its owner
    pub prev_2mb: *mut PageArrayElement,

    // Embedded state
    pub state: State,

    // tracks 4k pages in 2mb superpage
    pub count: i32,
}


// this is what the physical allocator is
pub struct PhysicalAllocator {
    pub free_4k_head: *mut PageArrayElement,
    pub free_2mb_head: *mut PageArrayElement,

    /// Pointer to start of the page_array metadata (so we can index into it)
    page_array_base: *mut PageArrayElement,
    page_array_len: usize,

    /// Optional: first usable physical page index (the index in page_array of first usable page)
    first_usable_page_index: usize,
    initialized: AtomicBool,
}


// this is a static reference to the physical allocator
// #[unsafe(no_mangle)]
// #[global_allocator]
// pub static mut KERNEL_PHYS_ALLOC: PhysicalAllocator = PhysicalAllocator::new_empty();
#[global_allocator]
pub static mut ALLOCATOR: PhysicalAllocator = PhysicalAllocator::new_empty();



// ------------ SATISFY GLOBALALLOC TRAIT FOR RUST ------------\\
// ------------ ---------------------------------- ------------\\
unsafe impl GlobalAlloc for PhysicalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // simple policy:
        // small allocations => 4K page
        // medium allocations (<= 2MB) => 2MB page
        // >2MB => null

        if layout.size() == 0 {
            return ptr::null_mut();
        }

        // If user asked for alignment > 4KiB, prefer 2MB if available
        if layout.size() > 4096 && layout.size() <= 2 * 1024 * 1024 {
            // allocate 2MB
            let allocator: &mut PhysicalAllocator = &mut ALLOCATOR;
            return allocator.alloc_2mb();
        }

        // anything <= 4k -> give a 4k page
        if layout.size() <= 4096 {
            let a = &mut ALLOCATOR;
            return a.alloc_4k();
        }

        // fallback: too large
        ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let allocator = &mut ALLOCATOR;

        // compute index to inspect metadata
        match allocator.index_for_ptr(ptr) {
            Some(idx) => {
                // inspect metadata to figure out if it's a 4k or 2mb allocation
                // add doesn't 'add' an element to the array
                // it simply advanced the pointer by <count> steps based on type 
                let p = allocator.page_array_base.add(idx);
                match (*p).state {
                    State::Alloc4K => allocator.free_4k(ptr),
                    State::Alloc2MB => allocator.free_2mb(ptr),
                    other => {
                        // double-free or invalid free — ignore or panic depending on policy
                        debug_assert!(false, "freeing pointer that is not allocated: state = {:?}", other);
                    }
                }
            }
            None => {
                // pointer outside physical coverage; ignore
            }
        }
    }
}


// -----------  PHYSICAL ALLOCATOR FUNCTIONS ---------- \\
// -----------  ----------------------------- ---------- \\
impl PhysicalAllocator {
 
    /// Create an empty allocator placeholder; 
    pub const fn new_empty() -> Self {
        Self {
            free_4k_head: ptr::null_mut(),
            free_2mb_head: ptr::null_mut(),
            page_array_base: ptr::null_mut(),
            page_array_len: 0,
            first_usable_page_index: 0,
            initialized: AtomicBool::new(false),
        }
    }

    /// Install the page_array metadata region so the allocator can operate.
    /// Call this at the end of your init_alloc (inside the same unsafe context) after you set up the page_array.
    pub unsafe fn install_page_array(
        &mut self,
        page_array_ptr: *mut PageArrayElement,
        page_array_len: usize,
        first_usable_page_index: usize,
    ) {
        self.page_array_base = page_array_ptr;
        self.page_array_len = page_array_len;
        self.first_usable_page_index = first_usable_page_index;

        // find first free 4k and first free 2mb head from the metadata
        self.free_4k_head = ptr::null_mut();
        self.free_2mb_head = ptr::null_mut(); 

        // scan metadata to set heads (first encountered free links)
        for i in 0..page_array_len {
            let p = page_array_ptr.add(i);
            match (*p).state {
                State::Free4K => {
                    if self.free_4k_head.is_null() {
                        self.free_4k_head = p;
                    }
                }
                State::Free2MB => {
                    // pick the head (count==512) as free_2mb_head
                    if (*p).count == 512 && self.free_2mb_head.is_null() {
                        self.free_2mb_head = p;
                    }
                }
                _ => {}
            }
        }
        self.initialized.store(true, Ordering::Release);
    }

    /// Convert a page index (0..N) into the PageArrayElement pointer
    #[inline(always)]
    unsafe fn page_element_for_index(&self, idx: usize) -> *mut PageArrayElement {
        self.page_array_base.add(idx)
    }

    /// Compute physical address (usize) of a page index
    /// NOTE: assumes physical page 0 starts at address 0 (physical address = idx * 4096)
    #[inline(always)]
    fn phys_for_index(&self, idx: usize) -> usize {
        idx * 4096
    }

    /// Convert a pointer returned to an index: assumes pointer is page-aligned and identity-mapped.
    /// Returns None if pointer is outside manageable range.
    #[inline(always)]
    fn index_for_ptr(&self, ptr: *mut u8) -> Option<usize> {

        // so cast the pointer as an address
        let addr = ptr as usize;

        // if the address % 4096 has a remainder, -> not aligned -> bad
        if addr % 4096 != 0 {
            return None;
        }

        // otherwise, it is a 4096byte aligned address and now we
        // simply check if its within bounds of page array (which it should be)
        let idx = addr / 4096;
        if idx < self.page_array_len {
            Some(idx)
        } else {
            None
        }
    }

    /// Allocate one 4KB page (returns physical pointer as *mut u8)
    pub unsafe fn alloc_4k(&mut self) -> *mut u8 {
        
        // serial_println!("alloc 4k starting");

        // If there is no more free 4kb pages on 4k list 
            if self.free_4k_head.is_null() {

                // No 2MB blocks left → Out of Memory
                if self.free_2mb_head.is_null() {
                    return ptr::null_mut();
                }

                // in this path, there is a 2mb page, so we will split it,
                // and return a 4kb page from the newly added set of pages
                    let old_2mb_head = self.free_2mb_head;
                    let new_2mb_head = (*old_2mb_head).next_2mb;

                    
                    // serial_println!("calling split 2mb");
                    // split the old 2mb_head
                    self.split_2mb(old_2mb_head);

                    // Unlink old from the 2MB free list
                    self.free_2mb_head = new_2mb_head;

                    // set the new heads previous to be null
                    // as long as there is a 2mb head
                    if !new_2mb_head.is_null() {
                        (*new_2mb_head).prev_2mb = ptr::null_mut();
                    }
            }

        // There is at least 1 4kb page on 4k list

        // Pop from 4K free list
        let alloc_4kb_page = self.free_4k_head;
        let next = (*alloc_4kb_page).next_4k;

        if !next.is_null() {
            (*next).prev_4k = ptr::null_mut();
        }

        self.free_4k_head = next;

        // Mark allocated
        (*alloc_4kb_page).state = State::Alloc4K;

        let idx = alloc_4kb_page.offset_from(self.page_array_base) as usize;

        // if this 4kb page was a subpage, decrement superpage free count
        let super_head = (*alloc_4kb_page).prev_2mb;
        if !super_head.is_null() && (*alloc_4kb_page).next_2mb.is_null(){
            (*super_head).count -= 1;
        }

        // return the 4kb page
        self.phys_for_index(idx) as *mut u8
}

    /// Free a single 4KB page given pointer (assumes identity mapping)
    pub unsafe fn free_4k(&mut self, pptr: *mut u8) {
        // serial_println!("free 4k starting");

        if !self.initialized.load(Ordering::Acquire) {
            return;
        }
        let idx = match self.index_for_ptr(pptr) {
            Some(i) => i,
            None => return,
        };
        let new_4k_head = self.page_array_base.add(idx);

        // sanity check
        debug_assert!((*new_4k_head).state == State::Alloc4K);

        // reach into page_array and mark this element as free_4k
        (*new_4k_head).state = State::Free4K;

        // get super_head reference from this element in the page array
        let super_head = (*new_4k_head).prev_2mb;

        // if this 4kb page is a subpage, its prev_2mb pointer should be init,
        // and its next will never be init
        // this is adjusting the superheads count eleemnt in the page_array -> correct
        if !super_head.is_null() && (*new_4k_head).next_2mb.is_null(){
            (*super_head).count += 1;

            // TODO: 
            // if the superpage hits 512 free 4kb pages
            // we should merge into a single 2mb page
            if (*super_head).count == 512 {
                self.merge_2mb(super_head);
            }
        }

        // push onto front of free_4k_head
            // set previous to be null -> this is new 4kb list head
            (*new_4k_head).prev_4k = ptr::null_mut();

            // set this new head's next ptr to the old free 4kb list head
            (*new_4k_head).next_4k = self.free_4k_head;

            // if there is a free 4kb head, set its previous to be our new head
            if !self.free_4k_head.is_null() {
                (*self.free_4k_head).prev_4k = new_4k_head;
            }

            // finally, set the 4kb head to be the new correct head
            self.free_4k_head = new_4k_head;
    }

    /// Allocate one 2MB superpage (returns physical pointer as *mut u8)
    pub unsafe fn alloc_2mb(&mut self) -> *mut u8 {

        // serial_println!("alloc 2mb starting");
        if self.free_2mb_head.is_null() {
            return ptr::null_mut();
        }

        // Pop head of 2MB list
        let head = self.free_2mb_head;
        let next = (*head).next_2mb;
        if !next.is_null() {
            (*next).prev_2mb = ptr::null_mut();
        }
        self.free_2mb_head = next;

        // Mark all 512 pages as Alloc2MB
        // head.index ... head.index + 511
        let head_idx = head.offset_from(self.page_array_base) as isize as usize;

        // TODO: this should just be 512
        let count = (*head).count as usize;
        debug_assert!(count == 512);

        for j in 0..count {
            let p = self.page_array_base.add(head_idx + j);
            (*p).state = State::Alloc2MB;
            // clear 4k links (so they won't be used in 4k list)
            (*p).next_4k = ptr::null_mut();
            (*p).prev_4k = ptr::null_mut();
        }

        self.phys_for_index(head_idx) as *mut u8
    }

    /// Free a 2MB superpage given pointer (assumes pointer is head of the 2MB region)
    pub unsafe fn free_2mb(&mut self, pptr: *mut u8) {

        
         serial_println!("free 2mb starting");

        if !self.initialized.load(Ordering::Acquire) {
            return;
        }
        let idx = match self.index_for_ptr(pptr) {
            Some(i) => i,
            None => return,
        };
        let head = self.page_array_base.add(idx);
        debug_assert!((*head).count as usize == 512);
        debug_assert!((*head).state == State::Alloc2MB);

        // mark all pages Free2MB
        for j in 0..512 {
            let p = self.page_array_base.add(idx + j);
        
            // deal with superpage
            if j == 0 {
                (*p).state = State::Free2MB;
                (*p).count = 512;

                // update the ptr to 2mb head
                    // current head == null
                    if(self.free_2mb_head.is_null()){
                        self.free_2mb_head = p;
                        (*p).prev_2mb = null_mut();
                        (*p).next_2mb = null_mut();
                    }
                    // current head is not null
                    else {
                        (*self.free_2mb_head).prev_2mb = p;
                        (*p).next_2mb = self.free_2mb_head;
                        self.free_2mb_head = p;
                    }
            }

            // deal with subpages
            else {
                (*p).state = State::Free2MB;
                (*p).next_4k = ptr::null_mut();
                (*p).prev_4k = ptr::null_mut();

            }
        }
    }


    // given ptr to a 2mb page, "merge" all 512 4kb pages into a single
    // 2mb page
    // remove all 512 from 4k list, add to 2mb list
    unsafe fn merge_2mb(&mut self, head_2mb: *mut PageArrayElement){
        
        // serial_println!("merge 2mb starting");

        if !self.initialized.load(Ordering::Acquire) {
            return;
        }
        let idx = head_2mb.offset_from(self.page_array_base) as usize;

        // ensure that it is currently on the 4kb list and 
        // its count is 512 free 4kb pages
        debug_assert!((*head_2mb).state == State::Free4K);
        debug_assert!((*head_2mb).count == 512);

        for j in 0..512 {

            // get to the jth subpage including the superpage element
            let p = self.page_array_base.add(idx + j);
            let current_2mb_head = self.free_2mb_head;

            // deal with super page element 
            if j == 0 {    
                // if current 2mb head is not null, make its prev point to our new 2mb head
                if !current_2mb_head.is_null(){
                    (*current_2mb_head).prev_2mb = p;
                    (*p).next_2mb = current_2mb_head;
                    self.free_2mb_head = p;
                }
                // if current 2mb head is null
                // this can be our new 2mb head
                else {
                    self.free_2mb_head = p;
                    (*p).prev_2mb = null_mut();
                    (*p).next_2mb = null_mut();
                }

                // in either case, we are merging a new 2mb page,
                // its state == free2mb && count == 512
                (*p).state = State::Free2MB;
                (*p).count = 512;

            }
            
            // deal with subpages
            if j != 0 {

                // ensure these elements are not on free 4kb list
                (*p).prev_4k = ptr::null_mut();
                (*p).next_4k = ptr::null_mut();

                // TODO: does their prev2mb reference need to be changed? 

            }

        }

    }

    // Split a 2mb page into 512 free 4kb pages
    unsafe fn split_2mb(&mut self, head_2mb: *mut PageArrayElement) {


        // serial_println!("split 2mb starting");
        if !self.initialized.load(Ordering::Acquire) {
            return;
        }

        // Compute index of the head metadata element
        let idx = head_2mb.offset_from(self.page_array_base) as usize;

        // serial_println!("state is {:?}", (*head_2mb).state);

        // Ensure we really are splitting a free 2MB block
        debug_assert!((*head_2mb).state == State::Free2MB);
        // serial_println!("got pass state assertion ");
        debug_assert!((*head_2mb).count == 512);
        // serial_println!("got pass count assertion ");

        // Each of the 512 pages becomes a free 4K page
        for j in 0..512 {

            // serial_println!("current j value is : {}  ", j);

            let p = self.page_array_base.add(idx + j);

            (*p).state = State::Free4K;

            if j != 0 {
                // All subpages point back to the superpage
                debug_assert!((*p).prev_2mb != ptr::null_mut());
            }

            // Push p onto the 4K free list
            if self.free_4k_head.is_null() {
                // first page created
                self.free_4k_head = p;
                (*p).prev_4k = ptr::null_mut();
                (*p).next_4k = ptr::null_mut();
            } else {
                // Insert at head
                (*p).next_4k = self.free_4k_head;
                (*self.free_4k_head).prev_4k = p;
                self.free_4k_head = p; 
                (*p).prev_4k = ptr::null_mut();
            }
        };

        // serial_println!("split 2mb finished");

    }



}

// -----------  END PHYSICAL ALLOCATOR FUNCTIONS ---------- \\
// -----------  ----====------------------------- ---------- \\



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
    let page_array: &mut [PageArrayElement] = unsafe {
        slice::from_raw_parts_mut(
            kernel_end() as *mut PageArrayElement,
            four_k_page_count as usize,
        )
    };

    // so i think the math which figures out how many useful pages there are is actually 
    // correct, which is excellent
    serial_println!("kernel_end() = {:#x}", kernel_end() as usize);
    serial_println!("page_array start = {:p}", page_array.as_ptr());
    serial_println!("page_array end   = {:p}", unsafe {
        page_array.as_ptr().add(page_array.len())
    });


    // we must ensure that we do not give away ptrs to memory and overwrite our page_array
    let page_array_size_bytes = four_k_page_count as usize * core::mem::size_of::<PageArrayElement>();
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
        let len_page_array_usize: usize = page_array.len();
        let not_useful_usize: usize = not_useful_pages_count as usize;
        let useful_usize: usize = len_page_array_usize - not_useful_usize;
        let remainder_usize: usize = remainder as usize;
        let mut counter_2mb_pages_init = 0;

        // Basic sanity check
        debug_assert!(page_array_pages as u64 + pages_up_to_kernel_end <= four_k_page_count);

        // 1) initialize all fields to safe defaults for entire array (no garbage)
        for idx in 0..len_page_array_usize {
            let p = page_array.as_ptr().add(idx) as *mut PageArrayElement;
            (*p).next_4k = ptr::null_mut();
            (*p).prev_4k = ptr::null_mut();
            (*p).next_2mb = ptr::null_mut();
            (*p).prev_2mb = ptr::null_mut();
            (*p).state = State::Unavail;
            (*p).count = 0;
        }

        // 2) mark pages before first usable page as Unavail and set 4k links there
        for idx in 0..not_useful_usize {
            // this line is nice because at first 'add(count)' looks weird, but it is 
            // rust compiler movign ptr ahead by count * sizeof(pointertype) which in this case
            // is page_array_element
            let p = page_array.as_ptr().add(idx) as *mut PageArrayElement;
            (*p).state = State::Unavail;
            (*p).prev_4k = if idx == 0 { ptr::null_mut() } else { page_array.as_ptr().add(idx - 1) as *mut _ };
            (*p).next_4k = if idx + 1 >= len_page_array_usize { ptr::null_mut() } else { page_array.as_ptr().add(idx + 1) as *mut _ };
        }

        // 3) Now the useful region: consume remainder (Free4K), then many 512-page superpages
        let mut idx = not_useful_usize;

        // consume remainder -> those useful pages that can't be part of a 2MB page
        let mut rem = remainder_usize;
        while rem > 0 && idx < len_page_array_usize {
            let p = page_array.as_ptr().add(idx) as *mut PageArrayElement;
            (*p).state = State::Free4K;
            (*p).prev_4k = if idx == 0 { ptr::null_mut() } else { page_array.as_ptr().add(idx - 1) as *mut _ };
            (*p).next_4k = if idx + 1 >= len_page_array_usize { ptr::null_mut() } else { page_array.as_ptr().add(idx + 1) as *mut _ };
            idx += 1;
            rem -= 1;
        }

        // carve full 2MB superpages (512 * 4KB)
        while idx + 512 <= len_page_array_usize {
            // head of superpage
            let head = page_array.as_ptr().add(idx) as *mut PageArrayElement;

            // set prev_2mb for head (null if this is the first)
            if counter_2mb_pages_init == 0 {
                (*head).prev_2mb = ptr::null_mut();
            } else {
                // previous head is at idx - 512
                (*head).prev_2mb = page_array.as_ptr().add(idx - 512) as *mut PageArrayElement;
                // also link previous head's next_2mb to this head
                let prev_head = page_array.as_ptr().add(idx - 512) as *mut PageArrayElement;
                (*prev_head).next_2mb = head;
            }

            // set head metadata
            (*head).state = State::Free2MB;
            (*head).count = 512;

            // initialize all 512 pages in the superpage
            for j in 0..512 {
                let pidx = idx + j;
                let p = page_array.as_ptr().add(pidx) as *mut PageArrayElement;

                // Mark all 512 entries as part of a 2MB block
                (*p).state = State::Free2MB;

                // Only the head stores the count
                (*p).count = if j == 0 { 512 } else { 0 };

                // These pages are NOT part of the free-4k list
                (*p).prev_4k = ptr::null_mut();
                (*p).next_4k = ptr::null_mut();

                // if we are not on the base index of the super page, it is a subpage
                if j != 0 {
                 // SUBPAGE: set its owning-superpage pointer
                    let head_ptr = page_array.as_ptr().add(idx) as *mut PageArrayElement;
                    (*p).prev_2mb = head_ptr;

                    // (its next_2mb must always be null)
                    (*p).next_2mb = ptr::null_mut();
                }
            }
            counter_2mb_pages_init += 1;
            idx += 512;
        }
    
        
    // ---- INSTALL the allocator metadata into the global allocator ----
    // Call install_page_array with three args: (ptr, len, first_usable_index)
    ALLOCATOR.install_page_array(
        page_array.as_mut_ptr(),
        page_array.len(),
        not_useful_usize,
    );
    serial_println!("free_4k_head = {:p}", ALLOCATOR.free_4k_head);
    serial_println!("free_2mb_head = {:p}", ALLOCATOR.free_2mb_head);
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

