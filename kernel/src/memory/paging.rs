use crate::info;
use limine::memory_map::{Entry, EntryType};
use spin::Mutex;
use x86_64::{
    VirtAddr,
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
};

pub static FRAME_ALLOCATOR: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);
pub static PAGE_TABLE: Mutex<Option<OffsetPageTable>> = Mutex::new(None);

/// A frame allocator that returns frames from the memory regions provided by the bootloader.
pub struct BootInfoFrameAllocator<'a> {
    memory_map: &'a [&'a Entry],
    next: usize,
}

impl BootInfoFrameAllocator<'_> {
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
unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

///
/// Initializes the global frame allocator using the provided memory map.
///
/// # Safety
/// The caller must ensure that the memory map is valid and not used elsewhere.
/// This function must only be called once, before any frame allocations occur.
pub unsafe fn init_frame_allocator(memory_map: &'static [&'static Entry]) {
    if FRAME_ALLOCATOR.lock().is_some() {
        panic!("Frame allocator already initialized");
    }
    FRAME_ALLOCATOR
        .lock()
        .replace(unsafe { BootInfoFrameAllocator::init(memory_map) });

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
