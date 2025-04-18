use limine::memory_map::{Entry, EntryType};
use x86_64::{
    VirtAddr,
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
};

/// A frame allocator that returns frames from the memory regions provided by the bootloader.
pub struct BootInfoFrameAllocator<'a> {
    memory_map: &'a [&'a Entry],
    next: usize,
}

impl<'a> BootInfoFrameAllocator<'a> {
    /// Initializes a new frame allocator with the given memory map.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the memory map is valid.
    pub unsafe fn init(memory_map: &'static [&Entry]) -> Self {
        Self {
            memory_map,
            next: 0,
        }
    }

    /// Returns an iterator over the usable frames specified in the memory map.
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let usable_regions = self
            .memory_map
            .iter()
            .filter(|region| region.entry_type == EntryType::USABLE);

        usable_regions
            .map(|region| region.base..(region.base + region.length))
            .flat_map(|region_range| region_range.step_by(4096))
            .map(|frame| PhysFrame::containing_address(x86_64::PhysAddr::new(frame)))
    }
}

/// Implement the FrameAllocator from `x86_64`` trait for BootInfoFrameAllocator.
unsafe impl<'a> FrameAllocator<Size4KiB> for BootInfoFrameAllocator<'a> {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

/// Initializes a new OffsetPageTable with the given memory offset.
///
/// # Safety
/// This function is unsafe because the caller must ensure that the memory offset is valid and that the virtual memory is mapped correctly.
pub unsafe fn init(memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = unsafe { get_level_4_table(memory_offset) };
    unsafe { OffsetPageTable::new(level_4_table, memory_offset) }
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
