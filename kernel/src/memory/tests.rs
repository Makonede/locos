use alloc::{boxed::Box, vec::Vec};

#[test_case]
fn test_simple_alloc() {
    let _x = Box::new(42);
}

#[test_case]
fn test_lots_of_pointers() {
    for i in 0..1000000 {
        let _x = Box::new(i);
    }
}

#[test_case]
fn test_big_heap_type() {
    let _x = Box::new([0u8; 1000000]);
}

#[test_case]
fn test_growing_vec() {
    let mut v = Vec::new();
    for i in 0..1000000 {
        v.push(i);
    }
}


