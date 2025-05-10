use core::ptr::NonNull;

use crate::{debug, info};
use limine::memory_map::{Entry, EntryType};
use spin::Mutex;
use x86_64::{
    structures::paging::{FrameAllocator, FrameDeallocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB}, PhysAddr, VirtAddr
};

pub static FRAME_ALLOCATOR: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);
pub static PAGE_TABLE: Mutex<Option<OffsetPageTable>> = Mutex::new(None);

/// A linked list of free frames.
#[derive(Clone, Copy, Debug)]
struct FrameFreeList {
    head: Option<NonNull<FrameNode>>,
    len: usize,
}

unsafe impl Send for FrameFreeList {}

impl FrameFreeList {
    /// Creates a new empty free list.
    const fn new() -> Self {
        FrameFreeList { head: None, len: 0 }
    }

    /// Pushes a frame onto the free list.
    const fn push(&mut self, ptr: NonNull<()>) {
        let node = ptr.cast::<FrameNode>();
        unsafe {
            node.write(FrameNode { next: self.head });
        }
        self.head = Some(node);
        self.len += 1;
    }

    /// Pops a frame from the free list.
    const fn pop(&mut self) -> Option<NonNull<()>> {
        if let Some(node) = self.head {
            self.head = unsafe { node.as_ref().next };
            self.len -= 1;
            Some(node.cast())
        } else {
            None
        }
    }

    const fn len(&self) -> usize {
        self.len
    }
}

/// A node in the linked list of free frames.
#[derive(Clone, Copy, Debug)]
struct FrameNode {
    next: Option<NonNull<FrameNode>>,
}

unsafe impl Send for FrameNode {}

/// A frame allocator that returns frames from the memory regions provided by the bootloader.
pub struct BootInfoFrameAllocator {
    memory_map: FrameFreeList,
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
            memory_map: FrameFreeList::new(),
            offset,
        };

        for frame in usable_regions {
            returned.memory_map.push(frame);
        }

        debug!("frame allocator initialized with {} frames", returned.memory_map.len());

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
        let ptr = NonNull::new(ptr as *mut ())
            .expect("failed to convert to NonNull");
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
