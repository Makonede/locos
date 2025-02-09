use x86_64::{
    VirtAddr,
    structures::paging::{OffsetPageTable, PageTable},
};

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