extern crate alloc;

use core::{alloc::GlobalAlloc, ptr::NonNull};

use crate::info;
use spin::Mutex;
use x86_64::{
    VirtAddr,
    structures::paging::{
        FrameAllocator, FrameDeallocator, Mapper, Page, PageTableFlags, Size4KiB,
        mapper::{MapToError, UnmapError},
    },
};

use super::{FRAME_ALLOCATOR, PAGE_TABLE};

pub static PAGE_ALLOCATOR: Mutex<PageAllocator> = Mutex::new(PageAllocator::new(
    VirtAddr::new(HEAP_START as u64),
    VirtAddr::new(HEAP_START as u64 + HEAP_SIZE as u64),
));

#[global_allocator]
pub static ALLOCATOR: Locked<BuddyAlloc<20, 16>> = Locked::new(BuddyAlloc::new(
    VirtAddr::new(HEAP_START as u64),
    VirtAddr::new(HEAP_START as u64 + HEAP_SIZE as u64),
));

pub const HEAP_START: usize = 0xFFFF_8800_0000_0000;
pub const HEAP_SIZE: usize = 8 * 1024 * 1024; // 8 MiB

/// Initialize a heap region in virtual memory and map it to physical frames
///
/// # Safety
/// This function is unsafe because the caller must guarantee that the
/// given memory region is unused and that the frame allocator is valid
pub unsafe fn init_heap() -> Result<(), MapToError<Size4KiB>> {
    let heap_start = Page::containing_address(VirtAddr::new(HEAP_START as u64));
    let heap_end = Page::containing_address(VirtAddr::new((HEAP_START + HEAP_SIZE - 1) as u64));

    // Map all pages in the heap
    for page in Page::range_inclusive(heap_start, heap_end) {
        let frame = FRAME_ALLOCATOR
            .lock()
            .as_mut()
            .unwrap()
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;

        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            PAGE_TABLE
                .lock()
                .as_mut()
                .unwrap()
                .map_to(page, frame, flags, FRAME_ALLOCATOR.lock().as_mut().unwrap())?
                .flush();
        }
    }

    info!("heap initialized: {:#?} - {:#?}", heap_start, heap_end);
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

/// A linked list of free memory blocks used in the buddy allocator
///
/// Each list tracks available blocks of a specific size, with nodes storing
/// only the next pointer to minimize overhead. The list is manipulated through
/// synchronized mutex access in the buddy allocator.
#[derive(Clone, Copy, Debug)]
struct FreeList {
    head: Option<NonNull<Node>>,
    len: usize,
}

impl FreeList {
    pub const fn new() -> Self {
        FreeList { head: None, len: 0 }
    }

    /// Pushes a new block onto the free list
    pub const fn push(&mut self, ptr: NonNull<()>) {
        let node = ptr.cast::<Node>();
        unsafe {
            node.write(Node { next: self.head });
        }
        self.head = Some(node);
        self.len += 1;
    }

    /// Pops a block from the free list
    pub const fn pop(&mut self) -> Option<NonNull<()>> {
        if let Some(node) = self.head {
            self.head = unsafe { node.as_ref().next };
            self.len -= 1;
            Some(node.cast())
        } else {
            None
        }
    }

    /// Checks if a block is in the free list
    ///
    /// This method takes O(n) time
    pub fn exists(&self, ptr: NonNull<()>) -> bool {
        let mut current = self.head;

        while let Some(node) = current {
            if node == ptr.cast::<Node>() {
                return true;
            }

            current = unsafe { node.as_ref().next };
        }

        false
    }

    /// Removes a block from the free list
    ///
    /// This method takes O(n) time
    pub fn remove(&mut self, ptr: NonNull<()>) {
        let mut current = self.head;
        let mut prev: Option<NonNull<Node>> = None;

        while let Some(node) = current {
            if node == ptr.cast::<Node>() {
                if let Some(mut prev) = prev {
                    unsafe {
                        prev.as_mut().next = node.as_ref().next;
                    }
                } else {
                    self.head = unsafe { node.as_ref().next };
                }
                self.len -= 1;
                return;
            }

            prev = current;
            current = unsafe { node.as_ref().next };
        }
    }

    #[expect(unused)]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[expect(unused)]
    pub const fn is_empty(&self) -> bool {
        self.head.is_none()
    }
}

/// A node in the free list containing just a next pointer
///
/// Nodes are embedded directly in the free memory blocks they represent,
/// allowing the memory to be reused when allocated.
#[derive(Clone, Copy, Debug)]
struct Node {
    next: Option<NonNull<Node>>,
}

// Safety: Node contains only a NonNull pointer which is used exclusively
// through synchronized mutex access in BuddyAlloc's implementation
unsafe impl Send for Node {}

/// A buddy allocator for managing heap memory allocations
///
/// The buddy allocator splits memory into power-of-two sized blocks, making it
/// efficient for allocating memory in small chunks while minimizing fragmentation.
///
/// # Type Parameters
/// * `L`: Number of levels in the buddy system
/// * `S`: Size of the smallest block in bytes
///
/// # Notes
/// * The allocator uses fixed-size arrays for free lists which trades some memory
///   overhead for implementation simplicity and deterministic performance.
/// * The number of possible blocks at the lowest level is 2^(L-1)
pub struct BuddyAlloc<const L: usize, const S: usize> {
    heap_start: VirtAddr,
    _heap_end: VirtAddr,
    free_lists: [FreeList; L],
}

// Safety: All access to internal data structures is protected by a Mutex
// in the Locked wrapper, ensuring thread-safe access to the allocator
unsafe impl<const L: usize, const S: usize> Send for BuddyAlloc<L, S> {}
unsafe impl<const L: usize, const S: usize> Sync for BuddyAlloc<L, S> {}

impl<const L: usize, const S: usize> BuddyAlloc<L, S> {
    /// Returns the number of possible blocks at the lowest level
    #[expect(unused)]
    const fn max_blocks() -> usize {
        1 << (L - 1)
    }

    /// Returns the maximum block size handled by this allocator
    const fn max_size() -> usize {
        S << (L - 1)
    }

    /// Returns the size of each block at a level
    pub const fn block_size(level: usize) -> usize {
        Self::max_size() >> level
    }

    /// Converts a block index to a pointer to the start of the block
    #[expect(unused)]
    const fn block_ptr(&self, level: usize, index: usize) -> NonNull<()> {
        let block_size = Self::block_size(level);
        let addr = self.heap_start.as_u64() as usize + (index * block_size);
        NonNull::new(addr as *mut ()).unwrap()
    }

    /// Creates a new buddy allocator with the given heap bounds
    ///
    /// # Arguments
    /// * `heap_start` - Virtual address of the heap start
    /// * `heap_end` - Virtual address of the heap end
    pub const fn new(heap_start: VirtAddr, _heap_end: VirtAddr) -> Self {
        let mut free_lists: [FreeList; L] = [FreeList::new(); L];
        free_lists[0].head = Some(
            NonNull::new(heap_start.as_u64() as *mut ())
                .unwrap()
                .cast::<Node>(),
        );
        free_lists[0].len = 1;

        Self {
            heap_start,
            _heap_end,
            free_lists,
        }
    }

    /// Determines the appropriate level for a requested allocation size
    ///
    /// Returns None if the requested size is larger than the maximum block size
    const fn get_level_from_size(&self, size: usize) -> Option<usize> {
        if size > Self::max_size() {
            return None;
        }

        let mut level = 1;
        while (Self::block_size(level)) >= size && level < L {
            level += 1;
        }

        Some(level - 1)
    }

    /// Attempts to get a free block at the specified level
    ///
    /// If no blocks are available at the requested level, attempts to split
    /// a larger block from a higher level
    fn get_free_block(&mut self, level: usize) -> Option<NonNull<()>> {
        if let Some(free_block) = self.free_lists[level].pop() {
            return Some(free_block);
        }
        self.split_level(level)
    }

    /// Splits a block from the next higher level to create two blocks at the current level
    ///
    /// Returns the index of the first block if successful, None if no higher level blocks
    /// are available
    fn split_level(&mut self, level: usize) -> Option<NonNull<()>> {
        if level == 0 {
            return None;
        }

        self.get_free_block(level - 1).inspect(|block| {
            let block_size = Self::block_size(level);
            let buddy = (block.as_ptr() as usize) ^ block_size;
            let buddy_ptr = NonNull::new(buddy as *mut ()).unwrap();

            self.free_lists[level].push(buddy_ptr);
        })
    }

    /// Recursively merges a freed block with its buddy if possible
    ///
    /// This helps prevent fragmentation by recombining adjacent free blocks into
    /// larger blocks when possible
    fn merge_buddies(&mut self, level: usize, ptr: NonNull<()>) {
        if level == 0 {
            self.free_lists[level].push(ptr);
            return;
        }

        let block_size = Self::block_size(level);
        let buddy = ptr.as_ptr() as usize ^ block_size;
        let buddy_nonnull = NonNull::new(buddy as *mut ()).unwrap();

        if self.free_lists[level].exists(buddy_nonnull) {
            // remove buddies from the free list
            self.free_lists[level].remove(buddy_nonnull);

            // add merged block to next level
            let first_buddy = core::cmp::min(ptr, buddy_nonnull);

            self.merge_buddies(level - 1, first_buddy);
        } else {
            self.free_lists[level].push(ptr);
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
unsafe impl<const L: usize, const S: usize> GlobalAlloc for Locked<BuddyAlloc<L, S>> {
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

        block.cast::<u8>().as_ptr()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        let mut inner = self.lock();
        let size = layout.size().next_power_of_two().max(layout.align());
        let level = match inner.get_level_from_size(size) {
            Some(l) => l,
            None => return,
        };

        inner.merge_buddies(level, NonNull::new(ptr as *mut ()).unwrap());
    }
}

/// Represents the layout of a page allocation
#[derive(Clone, Copy, Debug)]
pub struct PageAllocLayout {
    /// the page that represents the start of the allocation
    pub page: Page,
    /// the length of the allocation in pages
    pub length: usize,
}

impl PageAllocLayout {
    /// Creates a new page allocation layout
    pub const fn new(page: Page, length: usize) -> Self {
        PageAllocLayout { page, length }
    }
}

/// A wrapper around a buddy allocator to allocate pages.
///
/// 20 levels of buddy allocator with 4 KiB pages, meaning 2GiB of virtual memory
pub struct PageAllocator<const L: usize = 20> {
    allocator: BuddyAlloc<L, 4096>,
}

impl<const L: usize> PageAllocator<L> {
    /// Creates a new page allocator
    /// Start and end must be page aligned and match the size of the buddy allocator
    /// panics if the start and end are not page aligned
    pub const fn new(virt_start: VirtAddr, virt_end: VirtAddr) -> Self {
        if virt_start.as_u64() % 4096 != 0 {
            panic!("virt_start must be page aligned");
        } else if virt_end.as_u64() % 4096 != 0 {
            panic!("virt_end must be page aligned");
        }

        PageAllocator {
            allocator: BuddyAlloc::new(virt_start, virt_end),
        }
    }

    /// Maps a specified amount of contiguous pages using the global Physical Frame Allocator and Mapper
    pub fn allocate_pages(
        &mut self,
        num_pages: usize,
    ) -> Result<PageAllocLayout, MapToError<Size4KiB>> {
        let size = (num_pages * 4096).next_power_of_two();

        let level = self
            .allocator
            .get_level_from_size(size)
            .expect("Invalid size for page allocation");

        let block = self
            .allocator
            .get_free_block(level)
            .expect("OOM while allocating pages");

        let mut frame_alloc_lock = FRAME_ALLOCATOR.lock();
        let frame_alloc = frame_alloc_lock.as_mut().unwrap();
        let mut page_table_lock = PAGE_TABLE.lock();
        let page_table = page_table_lock.as_mut().unwrap();
        for page in ((block.as_ptr() as usize)..(block.as_ptr() as usize + size)).step_by(4096) {
            let physframe = frame_alloc
                .allocate_frame()
                .ok_or(MapToError::FrameAllocationFailed)?;

            unsafe {
                page_table
                    .map_to(
                        Page::containing_address(VirtAddr::new(page as u64)),
                        physframe,
                        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                        frame_alloc,
                    )?
                    .flush()
            };
        }

        Ok(PageAllocLayout::new(
            Page::containing_address(VirtAddr::new(block.as_ptr() as u64)),
            num_pages,
        ))
    }

    /// Deallocates an allocated amount of pages
    pub fn deallocate_pages(&mut self, info: PageAllocLayout) -> Result<(), UnmapError> {
        let size = (info.length * 4096).next_power_of_two();
        let level = self
            .allocator
            .get_level_from_size(size)
            .expect("Invalid size for page allocation");

        let mut frame_alloc_lock = FRAME_ALLOCATOR.lock();
        let frame_alloc = frame_alloc_lock.as_mut().unwrap();
        let mut page_table_lock = PAGE_TABLE.lock();
        let page_table = page_table_lock.as_mut().unwrap();

        for page in ((info.page.start_address().as_u64() as usize)
            ..(info.page.start_address().as_u64() as usize + size))
            .step_by(4096)
        {
            let (frame, flusher) = page_table.unmap(Page::<Size4KiB>::containing_address(
                VirtAddr::new(page as u64),
            ))?;
            unsafe { frame_alloc.deallocate_frame(frame) };
            flusher.flush();
        }

        self.allocator.merge_buddies(
            level,
            NonNull::new(info.page.start_address().as_u64() as *mut ()).unwrap(),
        );

        Ok(())
    }
}
