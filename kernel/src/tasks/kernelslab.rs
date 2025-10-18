use core::ptr::NonNull;

use spin::Mutex;
use x86_64::{
    VirtAddr,
    structures::paging::{
        FrameAllocator, Mapper, Page, PageTableFlags,
    },
};

use crate::{
    debug,
    memory::{FRAME_ALLOCATOR, PAGE_TABLE},
    tasks::scheduler::KSTACK_SIZE,
    trace,
};

pub static STACK_ALLOCATOR: Mutex<KernelSlabAlloc> = Mutex::new(KernelSlabAlloc::new());

/// Start address for kernel task stacks
const KERNEL_TASKS_START: u64 = 0xFFFF_F300_0000_0000;

/// slab allocator for kernel task stacks
///
/// supports max of 128 kernel tasks. Starts at KERNEL_TASKS_START
pub struct KernelSlabAlloc {
    block_bitmap: u128,
}

impl Default for KernelSlabAlloc {
    fn default() -> Self {
        KernelSlabAlloc::new()
    }
}

impl KernelSlabAlloc {
    pub const fn new() -> Self {
        KernelSlabAlloc { block_bitmap: 0 }
    }

    /// allocate a stack and guard page
    ///
    /// returns a pointer to the stack top (highest usable address)
    pub fn get_stack(&mut self) -> NonNull<()> {
        let block_index = self.block_bitmap.trailing_ones();

        trace!("block index is {}", block_index);

        assert!(block_index < 128, "No free kernel task blocks available");

        let block_start = KERNEL_TASKS_START + (block_index as u64 * KSTACK_SIZE as u64 * 0x1000);

        trace!("block start is {:#X}", block_start);

        let mut page_table_guard = PAGE_TABLE.lock();
        let page_table_lock = page_table_guard.as_mut().unwrap();

        // Map stack pages (skip the first page as guard page)
        for page_addr in
            (block_start + 0x1000..block_start + (KSTACK_SIZE as u64 * 0x1000)).step_by(0x1000)
        {
            unsafe {
                trace!("mapping page at {:#X}", page_addr);
                let frame = FRAME_ALLOCATOR
                    .lock()
                    .as_mut()
                    .unwrap()
                    .allocate_frame()
                    .expect("failed to allocate frame");
                page_table_lock
                    .map_to(
                        Page::containing_address(VirtAddr::new(page_addr)),
                        frame,
                        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                        FRAME_ALLOCATOR.lock().as_mut().unwrap(),
                    )
                    .expect("Failed to map page")
                    .flush();
            }
        }

        self.block_bitmap |= 1 << block_index;

        let stack_top = (block_start + (KSTACK_SIZE as u64 * 0x1000) - 1) & !0xF;
        debug!("Allocated stack at {:#x}", stack_top);
        NonNull::new(stack_top as *mut ()).unwrap()
    }

    /// deallocate a stack
    ///
    /// This does NOT unmap the pages or return frames to the allocator.
    /// The pages remain mapped but the block is marked as free for reuse.
    pub fn return_stack(&mut self, stack_top: NonNull<()>) {
        let stack_addr = stack_top.as_ptr() as u64;

        let offset = stack_addr - KERNEL_TASKS_START;
        let block_index = (offset & !(KSTACK_SIZE as u64 * 0x1000 - 1)) / (KSTACK_SIZE as u64 * 0x1000);

        assert!(block_index < 128 && (self.block_bitmap & (1 << block_index)) != 0);

        self.block_bitmap &= !(1 << block_index);
    }
}
