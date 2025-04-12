pub mod alloc;
pub mod paging;

pub use alloc::init_heap;
pub use paging::BootInfoFrameAllocator;
pub use paging::init;
