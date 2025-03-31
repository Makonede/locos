extern crate alloc;

use core::alloc::GlobalAlloc;

use x86_64::{
    VirtAddr,
    structures::paging::{
        FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, mapper::MapToError,
    },
};

#[global_allocator]
pub static ALLOCATOR: Locked<BuddyAlloc<14, 16, 8192>> = Locked::new(BuddyAlloc::new(
    VirtAddr::new(HEAP_START as u64),
    VirtAddr::new(HEAP_START as u64 + HEAP_SIZE as u64),
));

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
    let heap_end = Page::containing_address(VirtAddr::new((HEAP_START + HEAP_SIZE - 1) as u64));

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

/// A simple wrapper around spin::Mutex to provide safe interior mutability
pub struct Locked<A> {
    inner: spin::Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: spin::Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<A> {
        self.inner.lock()
    }
}

/// A buddy allocator for managing heap memory allocations
///
/// The buddy allocator splits memory into power-of-two sized blocks, making it
/// efficient for allocating memory in small chunks while minimizing fragmentation.
///
/// # Type Parameters
/// * `L`: Number of levels in the buddy system
/// * `S`: Size of the smallest block in bytes
/// * `N`: Maximum number of blocks at each level (fixed to avoid const generics)
///
/// # Notes
/// The allocator uses fixed-size arrays for free lists which trades some memory
/// overhead for implementation simplicity and deterministic performance.
pub struct BuddyAlloc<const L: usize, const S: usize, const N: usize> {
    heap_start: VirtAddr,
    _heap_end: VirtAddr,
    free_lists: [[usize; N]; L],
    counts: [usize; L],
}

impl<const L: usize, const S: usize, const N: usize> BuddyAlloc<L, S, N> {
    /// Returns the maximum block size handled by this allocator
    fn max_size() -> usize {
        S << (L - 1)
    }

    /// Creates a new buddy allocator with the given heap bounds
    ///
    /// # Arguments
    /// * `heap_start` - Virtual address of the heap start
    /// * `heap_end` - Virtual address of the heap end
    pub const fn new(heap_start: VirtAddr, _heap_end: VirtAddr) -> Self {
        let mut counts = [0; L];
        counts[0] = 1; // one free block at top level

        Self {
            heap_start,
            _heap_end,
            free_lists: [[0; N]; L],
            counts,
        }
    }

    /// Determines the appropriate level for a requested allocation size
    ///
    /// Returns None if the requested size is larger than the maximum block size
    fn get_level_from_size(&self, size: usize) -> Option<usize> {
        let max_size = Self::max_size();
        if size > max_size {
            return None;
        }

        let mut level = 1;
        while (max_size >> level) >= size && level < L {
            level += 1;
        }

        Some(level - 1)
    }

    /// Attempts to get a free block at the specified level
    ///
    /// If no blocks are available at the requested level, attempts to split
    /// a larger block from a higher level
    fn get_free_block(&mut self, level: usize) -> Option<usize> {
        if self.counts[level] != 0 {
            let free_block = Some(self.free_lists[level][self.counts[level] - 1]);
            self.counts[level] -= 1;
            return free_block;
        }
        self.split_level(level)
    }

    /// Splits a block from the next higher level to create two blocks at the current level
    ///
    /// Returns the index of the first block if successful, None if no higher level blocks
    /// are available
    fn split_level(&mut self, level: usize) -> Option<usize> {
        if level == 0 {
            return None;
        }

        self.get_free_block(level - 1).map(|block| {
            self.free_lists[level][self.counts[level]] = block * 2 + 1; // second block added to list
            self.counts[level] += 1;
            block * 2 // give first block
        })
    }

    /// Recursively merges a freed block with its buddy if possible
    ///
    /// This helps prevent fragmentation by recombining adjacent free blocks into
    /// larger blocks when possible
    fn merge_buddies(&mut self, level: usize, block: usize) {
        if level == 0 {
            return;
        }

        let buddy = block ^ 1;

        if let Some(index) = self.free_lists[level]
            .iter()
            .take(self.counts[level])
            .position(|&x| x == buddy)
        {
            // merge the buddy
            for i in index..self.counts[level] - 2 {
                self.free_lists[level][i] = self.free_lists[level][i + 1];
            }
            self.counts[level] -= 2;

            // add merged block to next level
            self.free_lists[level - 1][self.counts[level - 1]] = block / 2;
            self.counts[level - 1] += 1;

            self.merge_buddies(level - 1, block / 2);
        }
    }
}

/// Implementation of the global allocator interface for the buddy allocator
///
/// # Safety
/// The implementation guarantees that:
/// - Allocations are aligned to the requested alignment
/// - Each allocated block is exclusive and doesn't overlap with other allocations
/// - Deallocated blocks were previously allocated with the same layout
unsafe impl<const L: usize, const S: usize, const N: usize> GlobalAlloc
    for Locked<BuddyAlloc<L, S, N>>
{
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut inner = self.lock();
        let size = layout.size().next_power_of_two().max(layout.align());

        let level = match inner.get_level_from_size(size) {
            Some(l) => l,
            None => return core::ptr::null_mut(),
        };

        let block = match inner.get_free_block(level) {
            Some(b) => b,
            None => return core::ptr::null_mut(),
        };

        let block_size = BuddyAlloc::<L, S, N>::max_size() >> level;
        let addr = inner.heap_start.as_u64() as usize + (block * block_size);

        addr as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        let mut inner = self.lock();
        let size = layout.size().next_power_of_two().max(layout.align());
        let level = match inner.get_level_from_size(size) {
            Some(l) => l,
            None => return,
        };

        let offset = (ptr as usize) - inner.heap_start.as_u64() as usize;
        let block_size = BuddyAlloc::<L, S, N>::max_size() >> level;
        let block_index = offset / block_size;

        let last_free = inner.counts[level];
        inner.free_lists[level][last_free] = block_index;
        inner.counts[level] += 1;

        inner.merge_buddies(level, block_index);
    }
}
