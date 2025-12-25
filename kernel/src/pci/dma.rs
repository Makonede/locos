use alloc::vec::Vec;
use spin::{Lazy, Mutex};
use x86_64::{PhysAddr, VirtAddr};

use crate::memory::FRAME_ALLOCATOR;

pub(crate) static DMA_MANAGER: Lazy<Mutex<DmaManager>> =
    Lazy::new(|| Mutex::new(DmaManager::new().expect("DMA initialization failed (OOM)")));

#[derive(Debug, Clone, Copy)]
pub(crate) struct DmaError;

pub(crate) struct DmaManager {
    pub pools_4kb: DmaPool,
}

impl DmaManager {
    pub fn new() -> Result<Self, DmaError> {
        Ok(DmaManager {
            pools_4kb: DmaPool::new(1, 24)?,
        })
    }

    pub fn get_pool_4kb(&mut self) -> Option<DmaBuffer> {
        self.pools_4kb.allocate_buffer()
    }

    pub fn free_buffer_4kb(&mut self, buffer: DmaBuffer) {
        self.pools_4kb.free_buffer(buffer);
    }
}

/// Helper function for internal use during DmaPool initialization
pub(crate) fn get_zeroed_dma(frames: usize) -> Result<DmaBuffer, DmaError> {
    let mut lock = FRAME_ALLOCATOR.lock();
    let allocator = lock.as_mut().ok_or(DmaError)?;

    let virt = allocator
        .allocate_contiguous_pages(frames)
        .ok_or(DmaError)?;

    unsafe {
        core::ptr::write_bytes(virt.as_mut_ptr::<()>(), 0, frames * 4096);
    }

    let phys = PhysAddr::new(virt.as_u64() - allocator.hddm_offset);
    Ok(DmaBuffer {
        phys_addr: phys,
        virt_addr: virt,
        size: frames,
    })
}

pub(crate) unsafe fn free_zeroed_dma(buffer: DmaBuffer) -> Result<(), DmaError> {
    let mut lock = FRAME_ALLOCATOR.lock();
    let allocator = lock.as_mut().ok_or(DmaError)?;

    unsafe { allocator.deallocate_contiguous_frames(buffer.phys_addr, buffer.size) };

    Ok(())
}

pub(crate) struct DmaPool {
    buffers: Vec<DmaBuffer>,
    free_buffers: Vec<usize>,
    /// size in frames
    buffer_size: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DmaBuffer {
    pub phys_addr: PhysAddr,
    pub virt_addr: VirtAddr,
    /// size in frames
    pub size: usize,
}

impl DmaPool {
    pub fn new(buffer_size_frames: usize, num_buffers: usize) -> Result<Self, DmaError> {
        let mut buffers = Vec::with_capacity(num_buffers);
        for _ in 0..num_buffers {
            let buffer = get_zeroed_dma(buffer_size_frames)?;
            buffers.push(buffer);
        }

        let free_buffers = (0..num_buffers).collect();

        Ok(DmaPool {
            buffers,
            free_buffers,
            buffer_size: buffer_size_frames,
        })
    }

    pub fn allocate_buffer(&mut self) -> Option<DmaBuffer> {
        if let Some(index) = self.free_buffers.pop() {
            Some(self.buffers[index])
        } else {
            None
        }
    }

    pub fn free_buffer(&mut self, buffer: DmaBuffer) {
        if let Some(index) = self
            .buffers
            .iter()
            .position(|b| b.virt_addr == buffer.virt_addr)
        {
            self.free_buffers.push(index);
        }
    }
}
