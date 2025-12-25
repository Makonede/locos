//! Memory management for locOS.
//!
//! This module provides:
//! - Heap allocation via buddy allocator
//! - Page allocation and management
//! - Frame allocation from physical memory
//! - Page table management

pub mod alloc;
pub mod freelist;
pub mod paging;
pub mod tests;

pub use alloc::{init_heap, init_page_allocator};
pub use paging::FrameBuddyAllocatorForest;
pub use paging::{FRAME_ALLOCATOR, PAGE_TABLE, init, init_frame_allocator};
