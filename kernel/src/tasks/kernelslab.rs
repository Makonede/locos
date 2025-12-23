use spin::Mutex;
use x86_64::{
    VirtAddr,
    structures::paging::{
        FrameAllocator, FrameDeallocator, Mapper, Page, PageTableFlags,
    },
};

use crate::{
    debug,
    memory::{FRAME_ALLOCATOR, PAGE_TABLE},
    tasks::scheduler::{KSTACK_SIZE, UserInfo},
    trace, warn,
};

pub static STACK_ALLOCATOR: Mutex<KernelSlabAlloc> = Mutex::new(KernelSlabAlloc::new());

/// Start address for kernel task stacks
const KERNEL_TASKS_START: u64 = 0xFFFF_F300_0000_0000;
/// start of user stack region. grows downwards
pub const USER_STACKS_START: u64 = 0x0000_7fff_ffff_0000;
/// size of user stack in pages. Must be power of 2
pub const USTACK_SIZE: u64 = 512;
/// initial number of pages to allocate for user stack
pub const INITIAL_STACK_PAGES: u64 = 4;

#[derive(Debug, Clone, Copy)]
pub enum StackAllocError {
    FrameError,
    MapError,
}

impl core::fmt::Display for StackAllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StackAllocError::FrameError => write!(f, "Failed to allocate frame for stack"),
            StackAllocError::MapError => write!(f, "Failed to map stack page"),
        }
    }
}

impl core::error::Error for StackAllocError {}

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
    /// returns the address to the stack bottom (highest usable address)
    pub fn get_stack(&mut self) -> Result<VirtAddr, StackAllocError> {
        let block_index = self.block_bitmap.trailing_ones();

        trace!("block index is {}", block_index);

        if block_index >= 128 {
            return Err(StackAllocError::FrameError);
        }

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
                    .ok_or(StackAllocError::FrameError)?;
                page_table_lock
                    .map_to(
                        Page::containing_address(VirtAddr::new(page_addr)),
                        frame,
                        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                        FRAME_ALLOCATOR.lock().as_mut().unwrap(),
                    )
                    .map_err(|_| StackAllocError::MapError)?
                    .flush();
            }
        }

        self.block_bitmap |= 1 << block_index;

        let stack_top = (block_start + (KSTACK_SIZE as u64 * 0x1000) - 1) & !0xF;
        debug!("Allocated stack at {:#x}", stack_top);
        Ok(VirtAddr::new(stack_top))
    }

    /// deallocate a stack
    ///
    /// This does NOT unmap the pages or return frames to the allocator.
    /// The pages remain mapped but the block is marked as free for reuse.
    pub fn return_stack(&mut self, stack_top: VirtAddr) {
        let stack_addr = stack_top.as_u64();

        let offset = stack_addr - KERNEL_TASKS_START;
        let block_index = (offset & !(KSTACK_SIZE as u64 * 0x1000 - 1)) / (KSTACK_SIZE as u64 * 0x1000);

        assert!(block_index < 128 && (self.block_bitmap & (1 << block_index)) != 0);

        self.block_bitmap &= !(1 << block_index);
    }
}

/// Information about a user stack
/// 
/// stack_start: higher in memory start of stack
/// stack_end: lowest the stack can grow to
/// stack_size: size of stack in pages
#[derive(Debug, Clone, Copy)]
pub struct UserStackAllocation {
    pub stack_start: VirtAddr,
    pub stack_end: VirtAddr,
    pub stack_size: u64,
}

impl UserStackAllocation {
    pub fn new(stack_start: VirtAddr, stack_end: VirtAddr, stack_size: u64) -> Self {
        UserStackAllocation {
            stack_start,
            stack_end,
            stack_size,
        }
    }
}

pub fn get_user_stack(
    user_page_table: &mut x86_64::structures::paging::OffsetPageTable,
) -> Result<UserStackAllocation, StackAllocError> {
    let stack_end = USER_STACKS_START - (INITIAL_STACK_PAGES * 0x1000);

    trace!("user stack region: {:#X} - {:#X}", stack_end, USER_STACKS_START);

    trace!("Guard page at {:#X} (unmapped)", stack_end - 0x1000);

    for page_addr in (stack_end..USER_STACKS_START).step_by(0x1000) {
        unsafe {
            trace!("mapping initial user stack page at {:#X}", page_addr);
            let frame = FRAME_ALLOCATOR
                .lock()
                .as_mut()
                .unwrap()
                .allocate_frame()
                .ok_or(StackAllocError::FrameError)?;
            user_page_table
                .map_to(
                    Page::containing_address(VirtAddr::new(page_addr)),
                    frame,
                    PageTableFlags::PRESENT
                        | PageTableFlags::WRITABLE
                        | PageTableFlags::USER_ACCESSIBLE,
                    FRAME_ALLOCATOR.lock().as_mut().unwrap(),
                )
                .map_err(|_| StackAllocError::MapError)?
                .flush();
        }
    }

    // stack_end is already calculated correctly based on INITIAL_STACK_PAGES
    // 
    // The maximum stack can grow to is USER_STACKS_START - (USTACK_SIZE * 0x1000)
    let max_stack_end = USER_STACKS_START - (USTACK_SIZE * 0x1000);

    debug!(
        "Allocated user stack: top={:#x}, current_bottom={:#x}, max_bottom={:#x}, initial_size={} pages",
        USER_STACKS_START,
        stack_end,
        max_stack_end,
        INITIAL_STACK_PAGES
    );

    Ok(UserStackAllocation::new(VirtAddr::new(USER_STACKS_START), VirtAddr::new(max_stack_end), INITIAL_STACK_PAGES))
}

/// Deallocate a user stack by unmapping all pages and returning frames to the allocator
///
/// # Arguments
/// * `user_page_table` - The page table for the user task
/// * `stack_start` - The top of the stack (highest address)
/// * `stack_end` - The bottom of the stack (lowest address the stack can grow to)
/// * `stack_size` - The number of pages currently allocated for the stack
///
/// # Safety
/// The caller must ensure that:
/// - The stack_start, stack_end, and stack_size are valid and consistent
/// - The user_page_table is valid and corresponds to the task owning this stack
/// - No other references to the stack pages exist
pub unsafe fn return_user_stack(
    user_page_table: &mut x86_64::structures::paging::OffsetPageTable,
    UserInfo { stack_start, stack_end, stack_size, kernel_stack: _kernel_stack }: UserInfo,
) {
    let actual_stack_bottom = stack_start.as_u64() - (stack_size * 0x1000);

    trace!(
        "Deallocating user stack: start={:#x}, max_end={:#x}, actual_bottom={:#x}, size={} pages",
        stack_start,
        stack_end,
        actual_stack_bottom,
        stack_size,
    );

    for page_addr in (actual_stack_bottom..stack_start.as_u64()).step_by(0x1000) {
        let page = Page::containing_address(VirtAddr::new(page_addr));

        if let Ok((frame, flush)) = user_page_table.unmap(page) {
            flush.flush();
            unsafe {
                FRAME_ALLOCATOR
                    .lock()
                    .as_mut()
                    .unwrap()
                    .deallocate_frame(frame);
            }
            trace!("Unmapped and deallocated stack page at {:#x}", page_addr);
        } else {
            warn!("Failed to unmap stack page at {:#x}", page_addr);
        }
    }

    debug!("User stack deallocated successfully");
}
