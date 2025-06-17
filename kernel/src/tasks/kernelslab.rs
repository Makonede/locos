use core::ptr::NonNull;

use x86_64::{
    structures::paging::{frame, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB}, VirtAddr
};

use crate::{
    memory::{FRAME_ALLOCATOR, PAGE_TABLE},
    tasks::scheduler::KSTACK_SIZE,
};

/// Start address for kernel task stacks
const KERNEL_TASKS_START: u64 = 0xFFFF_F300_0000_0000;

/// slab allocator for kernel task stacks
///
/// supports max of 128 kernel tasks. Starts at KERNEL_TASKS_START
pub struct KernelSlabAlloc {
    block_bitmap: u128,
    block_mapped: u128,
}

impl Default for KernelSlabAlloc {
    fn default() -> Self {
        KernelSlabAlloc::new()
    }
}

impl KernelSlabAlloc {
    pub fn new() -> Self {
        KernelSlabAlloc {
            block_bitmap: 0,
            block_mapped: 0,
        }
    }

    /// allocate a stack and guard page
    /// 
    /// returns a pointer to the stack top (highest usable address)
    pub fn get_stack(&mut self) -> NonNull<()> {
        let block_index = self.block_bitmap.trailing_ones();
        
        if block_index >= 128 {
            panic!("Maximum number of kernel tasks exceeded");
        }
        
        let block_start = KERNEL_TASKS_START + (block_index as u64 * KSTACK_SIZE as u64 * 0x1000);
        
        if self.block_mapped & (1 << block_index) == 0 {
            // block not mapped yet
            let mut page_table_guard = PAGE_TABLE.lock();
            let page_table_lock = page_table_guard.as_mut().unwrap();
            
            // Map stack pages (skip the first page as guard page)
            for page_addr in block_start + 0x1000..block_start + (KSTACK_SIZE as u64 * 0x1000) {
                unsafe {
                    page_table_lock.map_to(
                        Page::containing_address(VirtAddr::new(page_addr)),
                        FRAME_ALLOCATOR.lock().as_mut().unwrap().allocate_frame().expect("Failed to allocate frame"),
                        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                        FRAME_ALLOCATOR.lock().as_mut().unwrap(),
                    ).expect("Failed to map page").flush();
                }
            }
            
            self.block_mapped |= 1 << block_index;
        }
        
        self.block_bitmap |= 1 << block_index;
        
        let stack_top = block_start + (KSTACK_SIZE as u64 * 0x1000);
        NonNull::new(stack_top as *mut ()).unwrap()
    }

    /// deallocate a stack and guard page
    pub fn return_stack(stack_start: NonNull<()>) {
        todo!()
    }
}
