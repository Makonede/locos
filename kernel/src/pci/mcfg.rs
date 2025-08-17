//! MCFG (Memory Mapped Configuration) table parsing and ECAM region management.
//!
//! This module handles:
//! - Parsing the ACPI MCFG table to discover PCIe configuration space regions
//! - Mapping ECAM (Enhanced Configuration Access Mechanism) regions to virtual memory
//! - Providing safe access to PCIe configuration space via memory-mapped I/O

use acpi::{AcpiTables, mcfg::Mcfg};
use alloc::vec::Vec;
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{Mapper, Page, PageTableFlags, PhysFrame, Size4KiB},
};

use crate::{
    debug, info,
    interrupts::apic::KernelAcpiHandler,
    memory::{FRAME_ALLOCATOR, PAGE_TABLE},
    warn,
};

use super::PciError;

/// Virtual address space start for ECAM mappings
const ECAM_VIRTUAL_START: u64 = 0xFFFF_F400_0000_0000;

/// Enhanced Configuration Access Mechanism region
#[derive(Debug, Clone, Copy)]
pub struct EcamRegion {
    /// Physical base address of the ECAM region
    pub base_address: PhysAddr,
    /// Virtual base address (mapped)
    pub virtual_address: VirtAddr,
    /// PCI segment group number
    pub segment_group: u16,
    /// Start bus number
    pub start_bus: u8,
    /// End bus number
    pub end_bus: u8,
}

impl EcamRegion {
    /// Calculate the virtual address for a specific bus/device/function
    /// Since the entire ECAM region is mapped, this gives direct access to any device's config space
    pub fn get_device_address(&self, bus: u8, device: u8, function: u8) -> VirtAddr {
        assert!(
            bus >= self.start_bus && bus <= self.end_bus,
            "Bus {} not in range {}-{}",
            bus,
            self.start_bus,
            self.end_bus
        );
        assert!(device < 32, "Device {device} out of range (0-31)");
        assert!(function < 8, "Function {function} out of range (0-7)");

        let bus_offset = (bus - self.start_bus) as u64;
        let offset = (bus_offset << 20) + ((device as u64) << 15) + ((function as u64) << 12);
        VirtAddr::new(self.virtual_address.as_u64() + offset)
    }

    /// Get the size of this ECAM region in bytes
    pub fn size(&self) -> u64 {
        self.mapping_size()
    }

    /// Get the total size needed for mapping (rounded up to cover full buses)
    pub fn mapping_size(&self) -> u64 {
        // Ensure end_bus >= start_bus to prevent underflow
        if self.end_bus < self.start_bus {
            warn!(
                "Invalid ECAM region: end_bus ({}) < start_bus ({})",
                self.end_bus, self.start_bus
            );
            return 0;
        }

        // Calculate bus count safely
        let bus_count = (self.end_bus as u64)
            .saturating_sub(self.start_bus as u64)
            .saturating_add(1);

        // Check for potential overflow before shifting
        if bus_count > (u64::MAX >> 20) {
            warn!("ECAM region too large: {} buses would overflow", bus_count);
            return u64::MAX;
        }

        bus_count << 20 // 1MB per bus
    }
}

/// Parse the ACPI MCFG table to discover ECAM regions
pub fn parse_mcfg_table(rsdp_addr: usize) -> Result<Vec<EcamRegion>, PciError> {
    let tables = unsafe {
        AcpiTables::from_rsdp(KernelAcpiHandler, rsdp_addr).map_err(|_| PciError::McfgNotFound)?
    };

    // Find the MCFG table
    let mcfg_table = tables
        .find_table::<Mcfg>()
        .map_err(|_| PciError::McfgNotFound)?;

    let mcfg = mcfg_table.get();

    debug!("MCFG table found with {} entries", mcfg.entries().len());

    let mut ecam_regions = Vec::new();

    // Parse each MCFG entry
    for entry in mcfg.entries() {
        // Copy packed struct fields to local variables to avoid unaligned references
        let base_address = entry.base_address;
        let pci_segment_group = entry.pci_segment_group;
        let bus_number_start = entry.bus_number_start;
        let bus_number_end = entry.bus_number_end;

        debug!(
            "MCFG entry: base={:#x}, segment={}, buses={}-{}",
            base_address,
            pci_segment_group,
            bus_number_start,
            bus_number_end
        );

        // Validate the ECAM entry
        if bus_number_end < bus_number_start {
            warn!(
                "Invalid MCFG entry: end_bus ({}) < start_bus ({}), skipping",
                bus_number_end, bus_number_start
            );
            continue;
        }

        if base_address == 0 {
            warn!("Invalid MCFG entry: base_address is 0, skipping");
            continue;
        }

        let ecam_region = EcamRegion {
            base_address: PhysAddr::new(base_address),
            virtual_address: VirtAddr::new(0),
            segment_group: pci_segment_group,
            start_bus: bus_number_start,
            end_bus: bus_number_end,
        };

        ecam_regions.push(ecam_region);
    }

    Ok(ecam_regions)
}

/// Check if a specific bus/device/function exists in any ECAM region
pub fn device_exists_in_regions(regions: &[EcamRegion], bus: u8, device: u8, function: u8) -> bool {
    for region in regions {
        if bus >= region.start_bus && bus <= region.end_bus {
            let vendor_id = read_config_u16(region, bus, device, function, 0x00);
            return vendor_id != 0xFFFF;
        }
    }
    false
}

/// Get the ECAM region that contains a specific bus
pub fn find_region_for_bus(regions: &[EcamRegion], bus: u8) -> Option<&EcamRegion> {
    regions
        .iter()
        .find(|region| bus >= region.start_bus && bus <= region.end_bus)
}

/// Calculate total memory usage for all ECAM regions
pub fn calculate_total_ecam_size(regions: &[EcamRegion]) -> u64 {
    regions.iter().map(|region| region.size()).sum()
}

/// Get the physical base address of an ECAM region
pub fn get_physical_base(region: &EcamRegion) -> PhysAddr {
    region.base_address
}

/// Get the virtual base address of an ECAM region
pub fn get_virtual_base(region: &EcamRegion) -> VirtAddr {
    region.virtual_address
}

/// Validate an ECAM region for potential issues
pub fn validate_ecam_region(region: &EcamRegion) -> Result<(), &'static str> {
    if region.end_bus < region.start_bus {
        return Err("end_bus < start_bus");
    }

    if region.base_address.as_u64() == 0 {
        return Err("base_address is 0");
    }

    if region.virtual_address.as_u64() == 0 {
        return Err("virtual_address not set");
    }

    let bus_count = (region.end_bus as u64)
        .saturating_sub(region.start_bus as u64)
        .saturating_add(1);
    if bus_count > 256 {
        return Err("too many buses (>256)");
    }

    Ok(())
}

/// Debug print ECAM region information
pub fn debug_ecam_region(region: &EcamRegion) {
    debug!(
        "ECAM Region: segment={}, buses={}-{}, phys={:#x}, virt={:#x}, size={}KB",
        region.segment_group,
        region.start_bus,
        region.end_bus,
        region.base_address.as_u64(),
        region.virtual_address.as_u64(),
        region.mapping_size() >> 10
    );

    validate_ecam_region(region).expect("ECAM region validation failed");
}

/// Map an entire ECAM region to virtual memory
/// This maps the complete PCIe configuration space for all buses in the region
pub fn map_ecam_region(region: &mut EcamRegion) -> Result<(), PciError> {
    static mut NEXT_ECAM_VIRT: u64 = ECAM_VIRTUAL_START;

    let mapping_size = region.mapping_size();

    // Check for zero size (invalid region)
    if mapping_size == 0 {
        return Err(PciError::EcamMappingFailed);
    }

    let pages_needed = mapping_size.div_ceil(0x1000);

    // Check for reasonable page count to prevent excessive memory usage
    if pages_needed > 1024 * 1024 {
        // Limit to 4GB of mapping
        warn!(
            "ECAM region requires {} pages ({}GB), this seems excessive",
            pages_needed,
            pages_needed >> 18
        );
        return Err(PciError::EcamMappingFailed);
    }

    unsafe {
        let virt_base = NEXT_ECAM_VIRT;

        // Check for virtual address space overflow
        if NEXT_ECAM_VIRT.saturating_add(pages_needed * 0x1000) < NEXT_ECAM_VIRT {
            warn!("Virtual address space overflow when mapping ECAM region");
            return Err(PciError::EcamMappingFailed);
        }

        NEXT_ECAM_VIRT += pages_needed * 0x1000;

        region.virtual_address = VirtAddr::new(virt_base);

        let mut page_table = PAGE_TABLE.lock();
        let page_table = page_table.as_mut().unwrap();
        let mut frame_allocator = FRAME_ALLOCATOR.lock();
        let frame_allocator = frame_allocator.as_mut().unwrap();

        info!(
            "Mapping entire ECAM region: phys={:#x} -> virt={:#x}, size={:#x} ({} pages)",
            region.base_address.as_u64(),
            region.virtual_address.as_u64(),
            mapping_size,
            pages_needed
        );

        for page_offset in 0..pages_needed {
            let virt_addr = VirtAddr::new(virt_base + page_offset * 0x1000);
            let phys_addr = PhysAddr::new(region.base_address.as_u64() + page_offset * 0x1000);

            let page = Page::<Size4KiB>::containing_address(virt_addr);
            let frame = PhysFrame::containing_address(phys_addr);

            let flags = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_CACHE
                | PageTableFlags::NO_EXECUTE;

            page_table
                .map_to(page, frame, flags, frame_allocator)
                .map_err(|_| PciError::EcamMappingFailed)?
                .flush();
        }
    }

    info!(
        "Successfully mapped ECAM region: buses {}-{}, {} MB of config space",
        region.start_bus,
        region.end_bus,
        mapping_size >> 20
    );

    Ok(())
}

/// Read a 32-bit value from PCIe configuration space
/// Returns the value read from the virtual address mapped configuration space
pub fn read_config_u32(region: &EcamRegion, bus: u8, device: u8, function: u8, offset: u16) -> u32 {
    assert!(
        offset % 4 == 0,
        "Config space offset must be 4-byte aligned"
    );
    assert!(offset < 4096, "Config space offset out of range");

    let device_base = region.get_device_address(bus, device, function);
    let address = device_base.as_u64() + offset as u64;

    unsafe { core::ptr::read_volatile(address as *const u32) }
}

/// Write a 32-bit value to PCIe configuration space
/// Writes to the virtual address mapped configuration space
pub fn write_config_u32(
    region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
    value: u32,
) {
    assert!(
        offset % 4 == 0,
        "Config space offset must be 4-byte aligned"
    );
    assert!(offset < 4096, "Config space offset out of range");

    let device_base = region.get_device_address(bus, device, function);
    let address = device_base.as_u64() + offset as u64;

    unsafe { core::ptr::write_volatile(address as *mut u32, value) }
}

/// Read a 16-bit value from PCIe configuration space
pub fn read_config_u16(region: &EcamRegion, bus: u8, device: u8, function: u8, offset: u16) -> u16 {
    assert!(
        offset % 2 == 0,
        "Config space offset must be 2-byte aligned"
    );
    assert!(offset < 4096, "Config space offset out of range");

    let device_base = region.get_device_address(bus, device, function);
    let address = device_base.as_u64() + offset as u64;

    unsafe { core::ptr::read_volatile(address as *const u16) }
}

/// Write a 16-bit value to PCIe configuration space
pub fn write_config_u16(
    region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
    value: u16,
) {
    assert!(
        offset % 2 == 0,
        "Config space offset must be 2-byte aligned"
    );
    assert!(offset < 4096, "Config space offset out of range");

    let device_base = region.get_device_address(bus, device, function);
    let address = device_base.as_u64() + offset as u64;

    unsafe { core::ptr::write_volatile(address as *mut u16, value) }
}

/// Read an 8-bit value from PCIe configuration space
pub fn read_config_u8(region: &EcamRegion, bus: u8, device: u8, function: u8, offset: u16) -> u8 {
    assert!(offset < 4096, "Config space offset out of range");

    let device_base = region.get_device_address(bus, device, function);
    let address = device_base.as_u64() + offset as u64;

    unsafe { core::ptr::read_volatile(address as *const u8) }
}

/// Write an 8-bit value to PCIe configuration space
pub fn write_config_u8(
    region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
    value: u8,
) {
    assert!(offset < 4096, "Config space offset out of range");

    let device_base = region.get_device_address(bus, device, function);
    let address = device_base.as_u64() + offset as u64;

    unsafe { core::ptr::write_volatile(address as *mut u8, value) }
}
