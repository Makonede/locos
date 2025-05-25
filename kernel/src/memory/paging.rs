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

pub static FRAME_ALLOCATOR: Mutex<Option<FrameBuddyAllocatorForest>> = Mutex::new(None);
pub static PAGE_TABLE: Mutex<Option<OffsetPageTable>> = Mutex::new(None);

/// A frame buddy allocator that manages multiple free lists for frames
/// N is the max number of levels, only adjustable at compile time
/// 
/// all methods work with virtual memory. It is assumed that there is an hddm offset present
/// and that the wrapper type handles the conversion.
pub struct FrameBuddyAllocator<const L: usize = 26> {
    free_lists: [FreeList; L],
    levels: usize,
    virt_start: usize,
    virt_end: usize,
}

impl<const L: usize> FrameBuddyAllocator<L> {
    /// Creates a new FrameBuddyAllocator with the specified levels, start, and end addresses.
    /// 
    /// # Safety
    /// Must be aligned to 4096 bytes (page size).
    /// Memory regions must be valid and not used elsewhere.
    pub const unsafe fn new(levels: usize, start: usize, end: usize) -> Self {
        assert!(
            levels >= 2,
            "buddy allocator needs at least 2 levels"
        );
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

        let mut free_lists = [FreeList::new(); L];

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

    /// Allocates a contiguous block of frames. Rounds up to the nearest power of two.
    pub fn allocate_contiguous_frames(&mut self, frames: usize) -> Option<u64> {
        let size = 4096 * frames;
        let level = self.get_level_from_size(size)?;
        let block = self.get_free_block(level)?;
        Some(block.as_ptr() as u64)
    }

    /// Deallocates a contiguous block of frames, merging with buddies if possible.
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

/// A forest of frame buddy allocators, each with its own free lists for different levels.
/// 
/// This allows for multiple independent allocators, each managing its own memory region.
/// N is the max number of possible allocators, only adjustable at compile time.
pub struct FrameBuddyAllocatorForest<const N: usize = 100, const L: usize = 26> {
    allocators: [Option<FrameBuddyAllocator<L>>; N],
    count: usize,
    hddm_offset: u64,
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
        let mut allocator_configs = [(0usize, 0usize, 0usize); N];
        let mut allocator_count = 0;
        
        for region in memory_regions {
            if region.entry_type != EntryType::USABLE {
                debug!("non usable");
                continue;
            }

            let start = region.base as usize;
            let length = region.length as usize;
            let end = start.checked_add(length)
                .expect("Memory region size causes overflow");

            let aligned_start = (start + 4095) & !4095;
            let aligned_end = end & !4095;
            
            if aligned_end <= aligned_start {
                continue;
            }
            
            let aligned_length = aligned_end - aligned_start;
            let total_frames = aligned_length / 4096;
            let mut current_start = aligned_start;
            let mut remaining_frames = total_frames;
            
            while remaining_frames >= min_allocator_frames {
                if allocator_count >= N {
                    panic!("Too many allocators needed, increase N parameter or use larger min_allocator_frames");
                }
                
                let mut allocator_frames = 1;
                while allocator_frames * 2 <= remaining_frames {
                    allocator_frames *= 2;
                }
                
                let allocator_size_bytes = allocator_frames.checked_mul(4096)
                    .expect("Allocator size calculation overflow");
                
                allocator_configs[allocator_count] = (current_start, allocator_frames, allocator_size_bytes);
                allocator_count += 1;
                
                current_start = current_start.checked_add(allocator_size_bytes)
                    .expect("Current start address overflow");
                remaining_frames -= allocator_frames;
            }
        }
        
        allocator_configs[..allocator_count].sort_unstable_by_key(|&(_, frames, _)| core::cmp::Reverse(frames));
        
        for &(start, frames, size_bytes) in allocator_configs.iter().take(allocator_count) {
            let virt_start = start + hddm_offset as usize;
            let virt_end = virt_start + size_bytes;
            let levels = if frames == 1 {
                1
            } else {
                frames.trailing_zeros() as usize + 1
            };
            
            if levels <= L {
                allocators[count] = Some(unsafe {
                    FrameBuddyAllocator::<L>::new(levels, virt_start, virt_end)
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

unsafe impl<const N: usize, const L: usize> FrameAllocator<Size4KiB> for FrameBuddyAllocatorForest<N, L> {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        for allocator in self.allocators[..self.count].iter_mut().flatten() {
            if let Some(virt_addr) = allocator.allocate_contiguous_frames(1) {
                let phys_addr = virt_addr - self.hddm_offset;
                return Some(PhysFrame::containing_address(PhysAddr::new(phys_addr)));
            }
        }
        None
    }
}

impl<const N: usize, const L: usize> FrameDeallocator<Size4KiB> for FrameBuddyAllocatorForest<N, L> {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame) {
        let phys_addr = frame.start_address().as_u64();
        let virt_addr = phys_addr + self.hddm_offset;
        let addr = virt_addr as usize;
        
        for allocator in self.allocators[..self.count].iter_mut().flatten() {
            if addr >= allocator.virt_start && addr < allocator.virt_end {
                unsafe { allocator.deallocate_contiguous_frames(virt_addr, 1) };
                return;
            }
        }
        panic!("Frame {:#x} not managed by any allocator", phys_addr);
    }
}

/// Initializes the global frame allocator using the provided memory map.
///
/// # Safety
/// The caller must ensure that the memory map is valid and not used elsewhere.
/// This function must only be called once, before any frame allocations occur.
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
