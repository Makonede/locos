use core::ptr::NonNull;

/// slab allocator for kernal task stacks
/// 
/// supports max of 128 kernel tasks.
/// 
/// 
pub struct KernelSlabAlloc {
    block_bitmap: u128,
}

impl KernelSlabAlloc {
    /// allocate a stack and guard page
    pub fn get_stack() -> NonNull<()> {
        todo!()
    }

    /// deallocate a stack and guard page
    pub fn return_stack(stack_start: NonNull<()>) {
        todo!()
    }
}