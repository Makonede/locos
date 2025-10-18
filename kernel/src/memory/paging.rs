use core::mem::{align_of, size_of};
use core::ptr::NonNull;

use crate::debug;
use crate::{
    info,
    memory::freelist::{DoubleFreeList, DoubleFreeListLink, DoubleFreeListNode},
};
use limine::memory_map::{Entry, EntryType};
use spin::Mutex;
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{
        FrameAllocator, FrameDeallocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB,
    },
};

pub static FRAME_ALLOCATOR: Mutex<Option<FrameBuddyAllocatorForest>> = Mutex::new(None);
pub static PAGE_TABLE: Mutex<Option<OffsetPageTable>> = Mutex::new(None);

/// statically fills the page list with entries
///
/// looks for the first place that can fill the page list.
///
/// # Safety
///
/// The caller must ensure that:
/// - The `entries` contain valid memory regions that are safe to write to
/// - The `hhdm_offset` correctly represents the higher half direct mapping offset
/// - The memory being written to is not currently in use by other system components
/// - No other code is concurrently accessing the same memory regions
pub unsafe fn fill_page_list(entries: &[&Entry], hhdm_offset: usize) {
    assert!(
        size_of::<DoubleFreeListNode>() <= 32,
        "DoubleFreeListNode must be 32 bytes or less"
    );
    assert!(
        align_of::<DoubleFreeListNode>() == 32,
        "DoubleFreeListNode must be aligned to 32 bytes"
    );

    for entry in entries {
        debug!(
            "Processing entry: base = {:#x}, length = {:#x}, type = {:?}",
            entry.base,
            entry.length,
            entry.entry_type == EntryType::USABLE
        );
        if !(entry.entry_type == EntryType::USABLE && entry.base != 0 && entry.length > 4096 * 4) {
            debug!("Skipping entry: not usable or too small");
            continue;
        }

        let entry_base = entry.base as usize + hhdm_offset;
        let needed_entries = entry.length as usize / 4096;

        (0..needed_entries).for_each(|i| {
            let offset = i * align_of::<DoubleFreeListNode>();
            let ptr =
                unsafe { (entry_base as *mut u8).add(offset) as usize } as *mut DoubleFreeListNode;
            unsafe {
                ptr.write(DoubleFreeListNode::new(
                    DoubleFreeListLink::new(None, None),
                    None,
                ));
            }
        });

        debug!(
            "wrote to page list at {:#x} with {} entries",
            entry_base, needed_entries
        );
    }
}

/// A frame buddy allocator that manages multiple free lists for frames
/// N is the max number of levels, only adjustable at compile time
///
/// all methods work with virtual memory. It is assumed that there is an hddm offset present
/// and that the wrapper type handles the conversion.
pub struct FrameBuddyAllocator<const L: usize = 26> {
    free_lists: [DoubleFreeList; L],
    levels: usize,
    virt_start: usize,
    virt_end: usize,
    page_list_start: usize,
}

unsafe impl<const L: usize> Send for FrameBuddyAllocator<L> {}

impl<const L: usize> FrameBuddyAllocator<L> {
    /// Creates a new FrameBuddyAllocator with the specified levels, start, and end addresses.
    ///
    /// # Safety
    /// Must be aligned to 4096 bytes (page size).
    /// Memory regions must be valid and not used elsewhere.
    pub unsafe fn new(levels: usize, start: usize, end: usize, page_list_start: usize) -> Self {
        assert!(levels >= 2, "buddy allocator needs at least 2 levels");
        assert!(start % 4096 == 0, "start must be page aligned");
        assert!(end % 4096 == 0, "end must be page aligned");
        let region_size = end - start;
        let expected_size = (1 << (levels - 1)) * 4096;
        assert!(
            region_size == expected_size,
            "region size does not match levels"
        );

        let mut free_lists = [DoubleFreeList::new(); L];

        debug!(
            "Creating FrameBuddyAllocator with {} levels, start: {:#x}, end: {:#x}, page_list_start: {:#x}",
            levels, start, end, page_list_start
        );
        let page_index: u128 = (start as u128 - page_list_start as u128) / 4096; // what page in the usable region is this?
        let page_ptr =
            page_index * align_of::<DoubleFreeListNode>() as u128 + page_list_start as u128; // ptr to start of managed location in local list
        debug!("Creating FrameBuddyAllocator page_ptr: {:#x}", page_ptr);

        free_lists[0].push(
            NonNull::new(page_ptr as *mut DoubleFreeListNode).unwrap(),
            region_size / 4096,
        );

        Self {
            free_lists,
            levels,
            virt_start: start,
            virt_end: end,
            page_list_start,
        }
    }

    /// Returns the block size for a given level in terms of number of pages.
    fn block_size(&self, level: usize) -> usize {
        let total_pages = (self.virt_end - self.virt_start) / 4096;
        total_pages >> level
    }

    /// Returns the smallest buddy level that can fit the requested size.
    fn get_level_from_size(&self, size: usize) -> Option<usize> {
        let mut level = 0;
        let mut block_size = self.virt_end - self.virt_start;
        while block_size > size && (level + 1) < self.levels {
            level += 1;
            block_size >>= 1;
        }
        if size > block_size { None } else { Some(level) }
    }

    /// Attempts to get a free block at the specified level, splitting higher blocks if needed.
    fn get_free_block(&mut self, level: usize) -> Option<NonNull<DoubleFreeListNode>> {
        if let Some(block) = self.free_lists[level].pop() {
            Some(block)
        } else {
            self.split_level(level)
        }
    }

    /// Splits a block from the next higher level to create two blocks at the current level.
    fn split_level(&mut self, level: usize) -> Option<NonNull<DoubleFreeListNode>> {
        if level == 0 {
            return None;
        }
        if let Some(block) = self.get_free_block(level - 1) {
            let block_size = self.block_size(level) * align_of::<DoubleFreeListNode>();
            let buddy_addr = (block.as_ptr() as usize) + block_size;
            let buddy_ptr = NonNull::new(buddy_addr as *mut DoubleFreeListNode).unwrap();
            self.free_lists[level].push(buddy_ptr, self.block_size(level));
            Some(block)
        } else {
            None
        }
    }

    /// Recursively merges a freed block with its buddy if possible, to reduce fragmentation.
    fn merge_buddies(&mut self, level: usize, ptr: NonNull<DoubleFreeListNode>) {
        if level == 0 {
            self.free_lists[level].push(ptr, self.block_size(level));
            return;
        }
        let block_size = self.block_size(level) * align_of::<DoubleFreeListNode>(); // in bytes
        let base = self.page_list_start;
        let offset = (ptr.as_ptr() as usize) - base;
        let buddy_offset = offset ^ block_size;
        let buddy_addr = base + buddy_offset;
        let buddy_ptr = NonNull::new(buddy_addr as *mut DoubleFreeListNode).unwrap();

        if unsafe { self.free_lists[level].contains(buddy_ptr, self.block_size(level)) } {
            unsafe { self.free_lists[level].remove(buddy_ptr) };
            let merged_ptr = if buddy_addr < ptr.as_ptr() as usize {
                buddy_ptr
            } else {
                ptr
            };
            self.merge_buddies(level - 1, merged_ptr);
        } else {
            self.free_lists[level].push(ptr, self.block_size(level));
        }
    }

    /// Allocates a contiguous block of frames. Rounds up to the nearest power of two.
    pub fn allocate_contiguous_frames(&mut self, frames: usize) -> Option<u64> {
        let size = 4096 * frames;
        let level = self.get_level_from_size(size)?;
        let block = self.get_free_block(level)?;

        let block_index =
            (block.as_ptr() as usize - self.page_list_start) / align_of::<DoubleFreeListNode>();
        let actual_page = self.page_list_start + block_index * 4096;

        Some(actual_page as u64)
    }

    /// Deallocates a contiguous block of frames, merging with buddies if possible.
    ///
    /// # Safety
    /// The caller must ensure that the block was allocated by this allocator and is not in use.
    pub unsafe fn deallocate_contiguous_frames(&mut self, addr: u64, frames: usize) {
        let page_index = (addr as usize - self.page_list_start) / 4096; // index from start of this region's pages list

        let ptr = NonNull::new(
            (self.page_list_start + page_index * align_of::<DoubleFreeListNode>())
                as *mut DoubleFreeListNode,
        )
        .unwrap();

        let size = 4096 * frames;
        let level = self.get_level_from_size(size).unwrap();
        self.merge_buddies(level, ptr);
    }
}

/// A forest of frame buddy allocators, each with its own free lists for different levels.
///
/// This allows for multiple independent allocators, each managing its own memory region.
/// N is the max number of possible allocators, only adjustable at compile time.
pub struct FrameBuddyAllocatorForest<const N: usize = 100, const L: usize = 26> {
    allocators: [Option<FrameBuddyAllocator<L>>; N],
    count: usize,
    pub hddm_offset: u64,
}

impl<const N: usize, const L: usize> FrameBuddyAllocatorForest<N, L> {
    pub fn init(memory_regions: &[&Entry], min_allocator_frames: usize, hddm_offset: u64) -> Self {
        if min_allocator_frames < 2 {
            panic!("min_allocator_frames must be at least 2 for buddy allocation");
        }
        if !min_allocator_frames.is_power_of_two() {
            panic!("min_allocator_frames must be a power of 2");
        }

        let mut allocators = [const { None }; N];
        let mut count = 0;
        let mut allocator_configs = [(0usize, 0usize, 0usize, 0usize); N]; // (virt_start, frames, size_bytes, list_start)
        let mut allocator_count = 0;

        for region in memory_regions {
            if region.entry_type != EntryType::USABLE {
                continue;
            }

            let start = region.base as usize;
            let length = region.length as usize;

            let total_frames = length / 4096;

            let pages_reserved_for_indexing =
                (total_frames * align_of::<DoubleFreeListNode>()).next_multiple_of(4096);

            let mut current_start = start + pages_reserved_for_indexing;
            let mut remaining_frames = total_frames - pages_reserved_for_indexing / 4096;

            while remaining_frames >= min_allocator_frames {
                if allocator_count >= N {
                    panic!(
                        "Too many allocators needed, increase N parameter or use larger min_allocator_frames"
                    );
                }

                let mut allocator_frames = 1;
                while allocator_frames * 2 <= remaining_frames {
                    allocator_frames *= 2;
                }

                let allocator_size_bytes = allocator_frames
                    .checked_mul(4096)
                    .expect("Allocator size calculation overflow");

                allocator_configs[allocator_count] = (
                    current_start,
                    allocator_frames,
                    allocator_size_bytes,
                    start + hddm_offset as usize,
                );
                allocator_count += 1;

                current_start = current_start
                    .checked_add(allocator_size_bytes)
                    .expect("Current start address overflow");
                remaining_frames -= allocator_frames;
            }
        }

        allocator_configs[..allocator_count]
            .sort_unstable_by_key(|&(_, frames, _, _)| core::cmp::Reverse(frames));

        for &(reg_start, frames, size_bytes, start) in
            allocator_configs.iter().take(allocator_count)
        {
            let virt_start = reg_start + hddm_offset as usize;
            let virt_end = virt_start + size_bytes;
            let levels = if frames == 1 {
                1
            } else {
                frames.trailing_zeros() as usize + 1
            };

            if levels <= L {
                allocators[count] = Some(unsafe {
                    FrameBuddyAllocator::<L>::new(levels, virt_start, virt_end, start)
                });
                count += 1;
            } else {
                panic!("Allocator requires {} levels but maximum is {}", levels, L);
            }
        }

        Self {
            allocators,
            count,
            hddm_offset,
        }
    }
}

impl<const N: usize, const L: usize> FrameBuddyAllocatorForest<N, L> {
    /// returns a virtual address the start of a contiguous block of frames
    #[inline]
    pub fn allocate_contiguous_pages(&mut self, pages: usize) -> Option<VirtAddr> {
        assert!(
            pages.is_power_of_two(),
            "Number of pages must be a power of two"
        );

        for allocator in self.allocators[..self.count].iter_mut().flatten() {
            if let Some(virt_addr) = allocator.allocate_contiguous_frames(pages) {
                return Some(VirtAddr::new(virt_addr));
            }
        }
        None
    }

    /// deallocates a contiguous block of frames at the given virtual address
    /// # Safety
    /// The caller must ensure that the address was allocated by this allocator and is not in use.
    #[inline]
    pub unsafe fn deallocate_contiguous_pages(&mut self, virt_addr: VirtAddr, pages: usize) {
        assert!(
            pages.is_power_of_two(),
            "Number of pages must be a power of two"
        );
        let addr = virt_addr.as_u64() as usize;

        for allocator in self.allocators[..self.count].iter_mut().flatten() {
            if addr >= allocator.virt_start && addr < allocator.virt_end {
                unsafe { allocator.deallocate_contiguous_frames(virt_addr.as_u64(), pages) };
                return;
            }
        }
        panic!("Address {:#x} not managed by any allocator", addr);
    }

    /// allocates contiguous physical frames
    pub fn allocate_contiguous_frames(&mut self, frames: usize) -> Option<PhysAddr> {
        assert!(
            frames.is_power_of_two(),
            "Number of frames must be a power of two"
        );

        self.allocate_contiguous_pages(frames).map(|virt_addr| {
            let phys_addr = virt_addr.as_u64() - self.hddm_offset;
            PhysAddr::new(phys_addr)
        })
    }

    /// deallocates contiguous physical frames
    ///
    /// # Safety
    /// The caller must ensure that the physical address was allocated by this allocator and is not in use.
    #[inline]
    pub unsafe fn deallocate_contiguous_frames(&mut self, phys_addr: PhysAddr, frames: usize) {
        assert!(
            frames.is_power_of_two(),
            "Number of frames must be a power of two"
        );

        let virt_addr = VirtAddr::new(phys_addr.as_u64() + self.hddm_offset);
        unsafe { self.deallocate_contiguous_pages(virt_addr, frames) };
    }
}

unsafe impl<const N: usize, const L: usize> FrameAllocator<Size4KiB>
    for FrameBuddyAllocatorForest<N, L>
{
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        self.allocate_contiguous_frames(1)
            .map(|phys_addr| PhysFrame::containing_address(phys_addr))
    }
}

impl<const N: usize, const L: usize> FrameDeallocator<Size4KiB>
    for FrameBuddyAllocatorForest<N, L>
{
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame) {
        let phys_addr = frame.start_address().as_u64();
        unsafe { self.deallocate_contiguous_frames(PhysAddr::new(phys_addr), 1) };
    }
}

/// Initializes the global frame allocator using the provided memory map.
///
/// # Safety
/// The caller must ensure that the memory map is valid and not used elsewhere.
/// This function must only be called once, before any frame allocations occur.
///
/// reserved_region is a tuple of (start, end) in bytes, which is reserved for the page list.
pub unsafe fn init_frame_allocator(memory_map: &'static [&'static Entry], hddm_offset: u64) {
    if FRAME_ALLOCATOR.lock().is_some() {
        panic!("Frame allocator already initialized");
    }

    let allocator = FrameBuddyAllocatorForest::init(memory_map, 0b10000, hddm_offset);
    FRAME_ALLOCATOR.lock().replace(allocator);

    info!("frame allocator initialized");
}

/// Initializes a new OffsetPageTable with the given memory offset.
///
/// # Safety
/// This function is unsafe because the caller must ensure that the memory offset is valid and that the virtual memory is mapped correctly.
pub unsafe fn init(memory_offset: VirtAddr) {
    let level_4_table = unsafe { get_level_4_table(memory_offset) };
    if PAGE_TABLE.lock().is_some() {
        panic!("Page table already initialized");
    }
    PAGE_TABLE
        .lock()
        .replace(unsafe { OffsetPageTable::new(level_4_table, memory_offset) });
    info!("page tables initialized");
}

/// Get a reference to the start of the level 4 page table in virtual memory.
///
/// # Safety
/// This function is unsafe because the caller must make sure there is a valid level 4 page table and the virtual memory is mapped correctly.
/// This function may only be called once to avoid multiple &mut references to the same data.
unsafe fn get_level_4_table(memory_offset: VirtAddr) -> &'static mut PageTable {
    let (level_4_table_frame, _) = x86_64::registers::control::Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = (phys.as_u64() + memory_offset.as_u64()) as *mut PageTable;
    unsafe { &mut *virt } // Waow, unsafe code!
}
