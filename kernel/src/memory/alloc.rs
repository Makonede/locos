extern crate alloc;

use x86_64::{
    structures::paging::{
        FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, mapper::MapToError,
    },
    VirtAddr,
};

pub const HEAP_START: usize = 0x_4444_0000_0000;
pub const HEAP_SIZE: usize = 128 * 1024; // 128 KiB

/// Initialize a heap region in virtual memory and map it to physical frames
///
/// # Safety
/// This function is unsafe because the caller must guarantee that the
/// given memory region is unused and that the frame allocator is valid
pub unsafe fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let heap_start = Page::containing_address(VirtAddr::new(HEAP_START as u64));
    let heap_end = Page::containing_address(VirtAddr::new(
        (HEAP_START + HEAP_SIZE - 1) as u64
    ));

    // Map all pages in the heap
    for page in Page::range_inclusive(heap_start, heap_end) {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
            
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    Ok(())
}

