pub mod alloc;
pub mod paging;

pub use alloc::init_heap;
pub use paging::BootInfoFrameAllocator;
pub use paging::{FRAME_ALLOCATOR, PAGE_TABLE, init, init_frame_allocator};
