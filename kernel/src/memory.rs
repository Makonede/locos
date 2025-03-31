pub mod paging;
pub mod alloc;

pub use paging::init;
pub use paging::BootInfoFrameAllocator;
pub use alloc::init_heap;