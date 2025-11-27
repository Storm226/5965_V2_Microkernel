use alloc::boxed::Box;
use alloc::vec::Vec;
use crate::{serial, serial_println};
use crate::memory::ALLOCATOR;
use core::alloc::{GlobalAlloc,Layout};
const KB4: usize = 4096;
const MB2: usize = 512 * 4096; // 2MB

// Allocate 4 KB
unsafe fn test_alloc_4k() -> *mut u8 {
    let layout = Layout::from_size_align(KB4, KB4).unwrap();
    ALLOCATOR.alloc(layout)
}

// Allocate 2 MB
unsafe fn test_alloc_2m() -> *mut u8 {
    let layout = Layout::from_size_align(MB2, MB2).unwrap();
    ALLOCATOR.alloc(layout)
}

// Free 4 KB memory
unsafe fn test_free_4k(ptr: *mut u8) {
    let layout = Layout::from_size_align(KB4, KB4).unwrap();
    ALLOCATOR.dealloc(ptr, layout)
}

// Free 2 MB memory
unsafe fn test_free_2m(ptr: *mut u8) {
    let layout = Layout::from_size_align(MB2, MB2).unwrap();
    ALLOCATOR.dealloc(ptr, layout)
}

fn simple_allocation() -> bool {
    let v1 = 41;
    let v2 = 33;
    let heap_value_1 = Box::new(v1);
    let heap_value_2 = Box::new(v2);

    if(*heap_value_1 != v1)
    {
        serial_println!("@ {} {} != {}", heap_value_1, *heap_value_1, v1);
        return false;
    }

    if(*heap_value_2 != v2)
    {
        serial_println!("@ {} {} != {}", heap_value_2, *heap_value_2, v2);
        return false;
    }

    return true;
}

// take a pointer 'page' and value 'v'
// then, dereference 'page' at offset
// compare value, if not equal,
unsafe fn check(page:*mut u8, v: u8, size: usize) -> bool
{

    serial_println!("checking page value {:?} v:{}" , page, v);
    for i in 0 .. size {
        let pv = *page.add(i);
        if(pv != v){
            serial_println!(" -> failed @{:?}[{}] = {}" , page, i, pv);
            return false;
        }
    }
    return true;
}


// take a pointer "page" and a value v, 
// dereference the pointer at an offset of 'i'
// and write into it v
// if size == 4096, write v 4096 times
unsafe fn write(page:*mut u8, v: u8, size: usize)
{
    for i in 0..size {
        *page.add(i) = v;
    }
}

unsafe fn test_allocator() -> bool {
    let page_sz_4k = 4096;
    let page_sz_2m = 4096*512;
    let v = 1;

    serial_println!("Allocate some pages...");


    // so get a pointer to a 4kb page\
    // write 4096 bytes to the 4096 page
    let test_page_4k = test_alloc_4k();
    serial_println!("alloc 4k @{:?}", test_page_4k);
    write(test_page_4k,v, page_sz_4k);




    // allocate a 2mb page, 
    // write 4096 * 512 bytes to it 
    // each byte simply increases in value 0 -> 4096 * 512
    let test_page_2m = test_alloc_2m();
    serial_println!("alloc 2m @{:?}", test_page_2m);
    write(test_page_2m,v, page_sz_2m);


    // iterate over the 4kb page, make sure you can correctly write
    if(!check(test_page_4k, v, page_sz_4k)){
        serial_println!("4k failed");
        return false;
    }

    // iterate over the 2mb page, make sure you can correctly write
    if(!check(test_page_2m, v, page_sz_2m)){
        serial_println!("2m failed");
        return false;
    }

    // 4096/8 = 512
    let arr_p= test_alloc_4k();
    let arr = arr_p as *mut *mut u8;

    let arr_p2= test_alloc_4k();
    let arr2 = arr_p2 as *mut *mut u8;

    serial_println!("Allocating 2mb as 512 4k...");
    for i in 0 .. 512 {
        let page = test_alloc_4k();
        //serial_println!("Page value as pointer : {:?}", page);
        write(page, 0xa, page_sz_4k);
        *arr.add(i) = page;
    }

    serial_println!("Allocating 2mb as 512 4k...");
    for i in 0 .. 512 {
        let page = test_alloc_4k();
        for j in 0 .. 512 {
            if(*arr.add(j) == page) 
            {
                serial_println!("allocator leaks, should never get {:?}", page);
                return false;
            }
        }
        write(page, 0xb, page_sz_4k);
        *arr2.add(i) = page;
    }

    for i in (0..512).step_by(255) {
        if !check(*arr.add(i), 0xa, page_sz_4k) {
            return false;
        }

        if !check(*arr2.add(i), 0xb, page_sz_4k) {
            return false;
        }
    }

    serial_println!("Freeing first half....");
    for i in 0 .. 256 {
        test_free_4k(*arr.add(i));
    }

    for i in 0 .. 256 {
        test_free_4k(*arr2.add(i));
    }

    serial_println!("Freeing second half....");
    for i in 256.. 512 {
        test_free_4k(*arr.add(i));
    }

    for i in 256 .. 512 {
        test_free_4k(*arr2.add(i));
    }    

    serial_println!("Checking values...");
    if(!check(test_page_4k, v, page_sz_4k)){
        serial_println!("4k failed after spray");
        return false;
    }

    if(!check(test_page_2m, v, page_sz_2m)){
        serial_println!("2m failed after spray");
        return false;
    }

    test_free_4k(test_page_4k);
    test_free_2m(test_page_2m);
    test_free_4k(arr_p);
    test_free_4k(arr_p2);


    return true;
}

fn large_vec() -> bool{
    let n = 1000;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    let sum = vec.iter().sum::<u64>();
    let expected = (n - 1) * n / 2;
    if (sum != expected)
    {
        serial_println!("vec expected: {}, got: {}", expected, sum);
        return false;
    }

    return true;
}

fn small_vec() -> bool{
    let n = 10;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    let sum = vec.iter().sum::<u64>();
    let expected = (n - 1) * n / 2;
    if (sum != expected)
    {
        serial_println!("vec expected: {}, got: {}", expected, sum);
        return false;
    }

    return true;
}

pub fn test_all()
{
    if(simple_allocation())
    {
        serial_println!("---- box test PASSED ----");
    }

    if(small_vec()) 
    {
        serial_println!("---- small vec test PASSED ----");
    }


    if(large_vec()) 
    {
        serial_println!("---- large vec test PASSED ----");
    }


    unsafe{
        if(test_allocator())
        {
            serial_println!("---- allocator test PASSED ----");
        }
    }
}