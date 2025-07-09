//! Virtual Memory Manager for PCIe device BARs
//!
//! This module provides a dedicated virtual memory allocator for mapping
//! PCIe device Base Address Registers (BARs) to virtual memory. It manages
//! a large contiguous virtual address space using a bitmap to track allocated pages.

use spin::Mutex;
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{Mapper, Page, PageTableFlags, PhysFrame, Size4KiB},
};

use crate::{
    info,
    memory::{FRAME_ALLOCATOR, PAGE_TABLE}, pci::device::MemoryBar,
};

use super::PciError;

/// Virtual address space start for PCIe BAR mappings
/// Moved further away from ECAM space to avoid collisions
const PCIE_VMM_START: u64 = 0xFFFF_FA00_0000_0000;
/// Size of the PCIe VMM region (16GB)
const PCIE_VMM_SIZE: u64 = 16 * 1024 * 1024 * 1024;
/// Page size (4KB)
const PAGE_SIZE: u64 = 4096;
/// Number of pages in the VMM region
const PCIE_VMM_PAGES: usize = (PCIE_VMM_SIZE / PAGE_SIZE) as usize;
/// Number of u128 words needed for the bitmap
const BITMAP_WORDS: usize = PCIE_VMM_PAGES.div_ceil(128);

/// Global PCIe VMM instance
pub static PCIE_VMM: Mutex<PcieVmm> = Mutex::new(PcieVmm::new());

/// PCIe Virtual Memory Manager
pub struct PcieVmm {
    /// Base virtual address of the managed region
    base_address: VirtAddr,
    /// Bitmap tracking allocated pages (1 = allocated, 0 = free)
    page_bitmap: [u128; BITMAP_WORDS],
    /// Next page to start searching from (for allocation optimization)
    next_search_start: usize,
}

/// Information about a mapped BAR
#[derive(Debug, Clone)]
pub struct MappedBar {
    /// Virtual address where the BAR is mapped
    pub virtual_address: VirtAddr,
    /// Physical address of the BAR
    pub physical_address: PhysAddr,
    /// Size of the mapped region in bytes
    pub size: u64,
    /// Whether the region is prefetchable
    pub prefetchable: bool,
}

impl PcieVmm {
    /// Create a new PCIe VMM instance
    pub const fn new() -> Self {
        Self {
            base_address: VirtAddr::new(PCIE_VMM_START),
            page_bitmap: [0u128; BITMAP_WORDS],
            next_search_start: 0,
        }
    }

    /// Map a memory BAR to virtual memory
    pub fn map_memory_bar(
        &mut self,
        physical_address: PhysAddr,
        size: u64,
        prefetchable: bool,
    ) -> Result<MappedBar, PciError> {
        if size == 0 || physical_address.as_u64() == 0 {
            return Err(PciError::InvalidDevice);
        }

        // Round up size to page boundary
        let pages_needed = size.div_ceil(PAGE_SIZE) as usize;
        
        // Find contiguous free pages
        let start_page = self.find_free_pages(pages_needed)
            .ok_or(PciError::AllocationFailed)?;

        // Calculate virtual address
        let virtual_address = VirtAddr::new(
            self.base_address.as_u64() + (start_page as u64 * PAGE_SIZE)
        );

        // Map the pages
        self.map_pages(virtual_address, physical_address, pages_needed, prefetchable)?;

        // Mark pages as allocated
        for i in start_page..(start_page + pages_needed) {
            self.set_page_allocated(i);
        }

        // Update search start hint
        self.next_search_start = start_page + pages_needed;
        if self.next_search_start >= PCIE_VMM_PAGES {
            self.next_search_start = 0;
        }

        info!(
            "Mapped PCIe BAR: phys={:#x} -> virt={:#x}, size={}KB{}",
            physical_address.as_u64(),
            virtual_address.as_u64(),
            size >> 10,
            if prefetchable { " (prefetchable)" } else { "" }
        );

        Ok(MappedBar {
            virtual_address,
            physical_address,
            size,
            prefetchable,
        })
    }

    /// Unmap a previously mapped BAR
    pub fn unmap_bar(&mut self, mapped_bar: &MappedBar) -> Result<(), PciError> {
        let pages_to_unmap = mapped_bar.size.div_ceil(PAGE_SIZE) as usize;
        let start_page = ((mapped_bar.virtual_address.as_u64() - self.base_address.as_u64()) / PAGE_SIZE) as usize;

        // Unmap the pages
        self.unmap_pages(mapped_bar.virtual_address, pages_to_unmap)?;

        // Mark pages as free
        for i in start_page..(start_page + pages_to_unmap) {
            if i < PCIE_VMM_PAGES {
                self.set_page_free(i);
            }
        }

        // Update search start hint if this frees earlier pages
        if start_page < self.next_search_start {
            self.next_search_start = start_page;
        }

        info!(
            "Unmapped PCIe BAR: virt={:#x}, size={}KB",
            mapped_bar.virtual_address.as_u64(),
            mapped_bar.size >> 10
        );

        Ok(())
    }

    /// Find contiguous free pages
    fn find_free_pages(&self, pages_needed: usize) -> Option<usize> {
        if pages_needed > PCIE_VMM_PAGES {
            return None;
        }

        // Start searching from the hint
        for start in self.next_search_start..=(PCIE_VMM_PAGES - pages_needed) {
            if self.is_range_free(start, pages_needed) {
                return Some(start);
            }
        }

        // Wrap around and search from the beginning
        (0..self.next_search_start.min(PCIE_VMM_PAGES - pages_needed + 1)).find(|&start| self.is_range_free(start, pages_needed))
    }

    /// Check if a range of pages is free
    fn is_range_free(&self, start: usize, count: usize) -> bool {
        for i in start..(start + count) {
            if i >= PCIE_VMM_PAGES || self.is_page_allocated(i) {
                return false;
            }
        }
        true
    }

    /// Set a page as allocated in the bitmap
    fn set_page_allocated(&mut self, page: usize) {
        if page < PCIE_VMM_PAGES {
            let word_index = page / 128;
            let bit_index = page % 128;
            debug_assert!(word_index < BITMAP_WORDS, "Word index {word_index} out of bounds");
            self.page_bitmap[word_index] |= 1u128 << bit_index;
        }
    }

    /// Set a page as free in the bitmap
    fn set_page_free(&mut self, page: usize) {
        if page < PCIE_VMM_PAGES {
            let word_index = page / 128;
            let bit_index = page % 128;
            debug_assert!(word_index < BITMAP_WORDS, "Word index {word_index} out of bounds");
            self.page_bitmap[word_index] &= !(1u128 << bit_index);
        }
    }

    /// Check if a page is allocated
    fn is_page_allocated(&self, page: usize) -> bool {
        if page >= PCIE_VMM_PAGES {
            return true; // Out of bounds = allocated
        }
        let word_index = page / 128;
        let bit_index = page % 128;
        debug_assert!(word_index < BITMAP_WORDS, "Word index {word_index} out of bounds");
        (self.page_bitmap[word_index] & (1u128 << bit_index)) != 0
    }

    /// Count the total number of allocated pages
    fn count_allocated_pages(&self) -> usize {
        let mut count = 0;

        // Count all complete words except the last one
        for i in 0..(BITMAP_WORDS - 1) {
            count += self.page_bitmap[i].count_ones() as usize;
        }

        // Handle the last word carefully to avoid counting excess bits
        let last_word_index = BITMAP_WORDS - 1;
        let last_word = self.page_bitmap[last_word_index];

        // Calculate how many valid bits are in the last word
        let total_bits = BITMAP_WORDS * 128;
        if total_bits > PCIE_VMM_PAGES {
            let valid_bits_in_last_word = 128 - (total_bits - PCIE_VMM_PAGES);
            let valid_mask = (1u128 << valid_bits_in_last_word) - 1;
            count += (last_word & valid_mask).count_ones() as usize;
        } else {
            count += last_word.count_ones() as usize;
        }

        count
    }

    /// Map physical pages to virtual pages
    fn map_pages(
        &self,
        virtual_address: VirtAddr,
        physical_address: PhysAddr,
        page_count: usize,
        prefetchable: bool,
    ) -> Result<(), PciError> {
        let mut page_table = PAGE_TABLE.lock();
        let mut frame_allocator = FRAME_ALLOCATOR.lock();

        // Set appropriate page flags for device memory
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;

        // For prefetchable memory, we can use write-through caching
        // For non-prefetchable memory, use uncacheable
        if !prefetchable {
            flags |= PageTableFlags::NO_CACHE;
        }

        if let (Some(page_table), Some(frame_allocator)) = (page_table.as_mut(), frame_allocator.as_mut()) {
            for i in 0..page_count {
                let virt_page: Page<Size4KiB> = Page::containing_address(
                    VirtAddr::new(virtual_address.as_u64() + (i as u64 * PAGE_SIZE))
                );
                let phys_frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(
                    PhysAddr::new(physical_address.as_u64() + (i as u64 * PAGE_SIZE))
                );

                unsafe {
                    page_table
                        .map_to(virt_page, phys_frame, flags, frame_allocator)
                        .map_err(|_| PciError::EcamMappingFailed)?
                        .flush();
                }
            }
        } else {
            return Err(PciError::EcamMappingFailed);
        }

        Ok(())
    }

    /// Unmap virtual pages
    fn unmap_pages(&self, virtual_address: VirtAddr, page_count: usize) -> Result<(), PciError> {
        let mut page_table = PAGE_TABLE.lock();

        if let Some(page_table) = page_table.as_mut() {
            for i in 0..page_count {
                let virt_page: Page<Size4KiB> = Page::containing_address(
                    VirtAddr::new(virtual_address.as_u64() + (i as u64 * PAGE_SIZE))
                );

                let (_frame, flush) = page_table
                    .unmap(virt_page)
                    .map_err(|_| PciError::EcamMappingFailed)?;
                flush.flush();
            }
        } else {
            return Err(PciError::EcamMappingFailed);
        }

        Ok(())
    }

    /// Get statistics about the VMM
    pub fn get_stats(&self) -> VmmStats {
        let allocated_pages = self.count_allocated_pages();
        let free_pages = PCIE_VMM_PAGES - allocated_pages;

        VmmStats {
            total_pages: PCIE_VMM_PAGES,
            allocated_pages,
            free_pages,
            total_size: PCIE_VMM_SIZE,
            allocated_size: allocated_pages as u64 * PAGE_SIZE,
            free_size: free_pages as u64 * PAGE_SIZE,
        }
    }
}

impl Default for PcieVmm {
    fn default() -> Self {
        Self::new()
    }
}

/// VMM statistics
#[derive(Debug, Clone)]
pub struct VmmStats {
    pub total_pages: usize,
    pub allocated_pages: usize,
    pub free_pages: usize,
    pub total_size: u64,
    pub allocated_size: u64,
    pub free_size: u64,
}

/// Map a BAR using the global VMM
/// Bar MUST be a memory BAR
pub fn map_bar(bar_info: &MemoryBar) -> Result<MappedBar, PciError> {
    let MemoryBar { address, size, prefetchable, .. } = bar_info;
    
    let mut vmm_lock = PCIE_VMM.lock();
    let mapped = vmm_lock.map_memory_bar(*address, *size, *prefetchable)?;
    Ok(mapped)
}

/// Find an existing mapping for a physical address (placeholder for now)
/// TODO: Implement proper mapping tracking in VMM
pub fn find_existing_mapping(_physical_address: PhysAddr) -> Result<Option<MappedBar>, PciError> {
    // For now, return None - this would require tracking all mappings in the VMM
    // In a full implementation, the VMM would maintain a hash map of physical->virtual mappings
    Ok(None)
}
