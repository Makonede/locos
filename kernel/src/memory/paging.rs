use core::ptr::NonNull;

use crate::{debug, info, memory::freelist::FreeList};
use limine::memory_map::{Entry, EntryType};
use spin::Mutex;
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{
        FrameAllocator, FrameDeallocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB,
    },
};

pub static FRAME_ALLOCATOR: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);
pub static PAGE_TABLE: Mutex<Option<OffsetPageTable>> = Mutex::new(None);

/// A frame buddy allocator that manages multiple free lists for frames
/// N is the max number of levels, only adjustable at compile time
/// 
/// all methods work with virtual memory. It is assumed that there is an hddm offset present
/// and that the wrapper type handles the conversion.
pub struct FrameBuddyAllocator<const N: usize = 26> {
    free_lists: [FreeList; N],
    levels: usize,
    virt_start: usize,
    virt_end: usize,
}

impl<const N: usize> FrameBuddyAllocator<N> {
    /// Creates a new FrameBuddyAllocator with the specified levels, start, and end addresses.
    /// 
    /// # Safety
    /// Must be aligned to 4096 bytes (page size).
    /// Memory regions must be valid and not used elsewhere.
    pub const unsafe fn new(levels: usize, start: usize, end: usize) -> Self {
        assert!(
            start % 4096 == 0,
            "start must be page aligned"
        );
        assert!(
            end % 4096 == 0,
            "end must be page aligned"
        );
        let region_size = end - start;
        let expected_size = (1 << (levels - 1)) * 4096;
        assert!(
            region_size == expected_size,
            "region size does not match levels"
        );

        let mut free_lists = [FreeList::new(); N];

        free_lists[0].push(NonNull::new(start as *mut ()).unwrap());

        Self {
            free_lists,
            levels,
            virt_start: start,
            virt_end: end,
        }
    }

    /// Returns the block size for a given level.
    fn block_size(&self, level: usize) -> usize {
        (self.virt_end - self.virt_start) >> level
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
    fn get_free_block(&mut self, level: usize) -> Option<NonNull<()>> {
        if let Some(block) = self.free_lists[level].pop() {
            Some(block)
        } else {
            self.split_level(level)
        }
    }

    /// Splits a block from the next higher level to create two blocks at the current level.
    fn split_level(&mut self, level: usize) -> Option<NonNull<()>> {
        if level == 0 {
            return None;
        }
        if let Some(block) = self.get_free_block(level - 1) {
            let block_size = self.block_size(level);
            let buddy_addr = (block.as_ptr() as usize) + block_size;
            let buddy_ptr = NonNull::new(buddy_addr as *mut ()).unwrap();
            self.free_lists[level].push(buddy_ptr);
            Some(block)
        } else {
            None
        }
    }

    /// Recursively merges a freed block with its buddy if possible, to reduce fragmentation.
    fn merge_buddies(&mut self, level: usize, ptr: NonNull<()>) {
        if level == 0 {
            self.free_lists[level].push(ptr);
            return;
        }
        let block_size = self.block_size(level);
        let base = self.virt_start;
        let offset = (ptr.as_ptr() as usize) - base;
        let buddy_offset = offset ^ block_size;
        let buddy_addr = base + buddy_offset;
        let buddy_ptr = NonNull::new(buddy_addr as *mut ()).unwrap();

        if self.free_lists[level].exists(buddy_ptr) {
            self.free_lists[level].remove(buddy_ptr);
            let merged_ptr = if buddy_addr < ptr.as_ptr() as usize {
                buddy_ptr
            } else {
                ptr
            };
            self.merge_buddies(level - 1, merged_ptr);
        } else {
            self.free_lists[level].push(ptr);
        }
    }

    /// Allocates a contiguous block of 2^order frames (order=0 means 1 frame).
    pub fn allocate_contiguous_frames(&mut self, frames: usize) -> Option<u64> {
        let size = 4096 * frames;
        let level = self.get_level_from_size(size)?;
        let block = self.get_free_block(level)?;
        Some(block.as_ptr() as u64)
    }

    /// Deallocates a contiguous block of 2^order frames.
    /// 
    /// # Safety
    /// The caller must ensure that the block was allocated by this allocator and is not in use.
    pub unsafe fn deallocate_contiguous_frames(&mut self, addr: u64, frames: usize) {
        let ptr = NonNull::new(addr as *mut ()).unwrap();
        let size = 4096 * frames;
        let level = self.get_level_from_size(size).unwrap();
        self.merge_buddies(level, ptr);
    }
}

/// A frame allocator that returns frames from the memory regions provided by the bootloader.
pub struct BootInfoFrameAllocator {
    memory_map: FreeList,
    offset: u64,
}

impl BootInfoFrameAllocator {
    /// Initializes a new frame allocator with the given memory map.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the memory map is valid.
    pub unsafe fn init(memory_map: &'static [&Entry], offset: u64) -> Self {
        let usable_regions = memory_map
            .iter()
            .filter(|region| region.entry_type == EntryType::USABLE)
            .map(|region| region.base..(region.base + region.length))
            .flat_map(|region_range| region_range.step_by(4096))
            .map(|base| base + offset)
            .map(|frame| unsafe { NonNull::new_unchecked(frame as *mut ()) });

        let mut returned = Self {
            memory_map: FreeList::new(),
            offset,
        };

        for frame in usable_regions {
            returned.memory_map.push(frame);
        }

        debug!(
            "frame allocator initialized with {} frames",
            returned.memory_map.len()
        );

        returned
    }
}

/// Implement the FrameAllocator from `x86_64`` trait for BootInfoFrameAllocator.
unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        if let Some(ptr) = self.memory_map.pop() {
            let phys_ptr = PhysAddr::new((ptr.as_ptr() as u64) - self.offset); // we ball baby
            let frame = PhysFrame::containing_address(phys_ptr);
            Some(frame)
        } else {
            None
        }
    }
}

impl FrameDeallocator<Size4KiB> for BootInfoFrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame) {
        let ptr = frame.start_address().as_u64() + self.offset;
        let ptr = NonNull::new(ptr as *mut ()).expect("failed to convert to NonNull");
        self.memory_map.push(ptr);
    }
}

/// Initializes the global frame allocator using the provided memory map.
///
/// # Safety
/// The caller must ensure that the memory map is valid and not used elsewhere.
/// This function must only be called once, before any frame allocations occur.
pub unsafe fn init_frame_allocator(memory_map: &'static [&'static Entry], offset: u64) {
    if FRAME_ALLOCATOR.lock().is_some() {
        panic!("Frame allocator already initialized");
    }
    FRAME_ALLOCATOR
        .lock()
        .replace(unsafe { BootInfoFrameAllocator::init(memory_map, offset) });

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
