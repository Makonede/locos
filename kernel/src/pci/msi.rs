//! MSI-X interrupt handling for PCIe devices.
//!
//! This module provides:
//! - MSI-X (Extended Message Signaled Interrupts) setup and management
//! - Interrupt vector allocation and routing
//! - Device interrupt configuration
//! 
//! 
//! NOTE: only delivers to core 0.

use core::ptr::write_bytes;

use alloc::vec::Vec;

use crate::{info, warn};

use super::{
    PciError,
    config::{
        MsiXTableEntry, capability_ids, msix_control_bits,
        msix_offsets,
    },
    device::PciDevice,
    mcfg::{read_config_u16, read_config_u32, write_config_u16},
};

/// MSI-X virtual address space start
#[allow(dead_code)]
const MSIX_VIRTUAL_START: u64 = 0xFFFF_F500_0000_0000;

/// MSI-X interrupt information
#[derive(Debug, Clone)]
pub struct MsiXInfo {
    /// Device that owns this MSI-X
    pub device: PciDevice,
    /// Capability offset in configuration space
    pub cap_offset: u16,
    /// Number of table entries
    pub table_size: u16,
    /// Table BAR index
    pub table_bar: u8,
    /// Table offset within BAR
    pub table_offset: u32,
    /// PBA (Pending Bit Array) BAR index
    pub pba_bar: u8,
    /// PBA offset within BAR
    pub pba_offset: u32,
    /// Virtual address of mapped table
    pub table_virtual_addr: Option<u64>,
    /// Virtual address of mapped PBA
    pub pba_virtual_addr: Option<u64>,
    /// Allocated interrupt vectors
    pub vectors: Vec<MsiXVector>,
    /// Mapped BARs for this device
    pub mapped_bars: Vec<super::vmm::MappedBar>,
}

/// MSI-X vector information
#[derive(Debug, Clone)]
pub struct MsiXVector {
    /// Vector index in the table
    pub index: u16,
    /// Interrupt vector number
    pub vector: u8,
    /// Whether this vector is enabled
    pub enabled: bool,
}

impl MsiXInfo {
    /// Create MSI-X information from a device capability
    pub fn from_device(device: &PciDevice, cap_offset: u16) -> Result<Self, PciError> {
        let control = read_config_u16(
            &device.ecam_region,
            device.bus,
            device.device,
            device.function,
            cap_offset + msix_offsets::MESSAGE_CONTROL,
        );
        let table_offset_bir = read_config_u32(
            &device.ecam_region,
            device.bus,
            device.device,
            device.function,
            cap_offset + msix_offsets::TABLE_OFFSET_BIR,
        );
        let pba_offset_bir = read_config_u32(
            &device.ecam_region,
            device.bus,
            device.device,
            device.function,
            cap_offset + msix_offsets::PBA_OFFSET_BIR,
        );

        let table_size = (control & msix_control_bits::TABLE_SIZE_MASK) + 1;
        let table_bar = (table_offset_bir & 0x7) as u8;
        let table_offset = table_offset_bir & !0x7;
        let pba_bar = (pba_offset_bir & 0x7) as u8;
        let pba_offset = pba_offset_bir & !0x7;

        Ok(Self {
            device: device.clone(),
            cap_offset,
            table_size,
            table_bar,
            table_offset,
            pba_bar,
            pba_offset,
            table_virtual_addr: None,
            pba_virtual_addr: None,
            vectors: Vec::new(),
            mapped_bars: Vec::new(),
        })
    }



    fn map_device_bars(&mut self) -> Result<(), PciError> {
        for bar in self.device.bars.iter() {
            let super::device::BarInfo::Memory(memory_bar) = bar else {
                continue;
            };

            if memory_bar.address.as_u64() == 0 {
                continue;
            }

            let address = memory_bar.address;

            let already_mapped = self.mapped_bars
                .iter()
                .any(|mapped| mapped.physical_address == address);

            if already_mapped {
                continue;
            }

            match super::vmm::map_bar(memory_bar) {
                Ok(mapped) => {
                    info!(
                        "MSI-X mapped BAR: phys={:#x} -> virt={:#x}",
                        mapped.physical_address.as_u64(),
                        mapped.virtual_address.as_u64()
                    );
                    self.mapped_bars.push(mapped);
                }
                Err(e) => {
                    if let Some(existing_mapping) = super::vmm::find_existing_mapping(address)? {
                        self.mapped_bars.push(existing_mapping);
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    /// Find the virtual address for a specific BAR index
    fn find_bar_virtual_address(&self, bar_index: u8) -> Result<u64, PciError> {
        let Some(bar_info) = self.device.bars.get(bar_index as usize) else {
            return Err(PciError::InvalidDevice);
        };

        let super::device::BarInfo::Memory(memory_bar) = bar_info else {
            return Err(PciError::InvalidDevice);
        };

        // Find the corresponding mapped BAR
        for mapped in &self.mapped_bars {
            if mapped.physical_address == memory_bar.address {
                return Ok(mapped.virtual_address.as_u64());
            }
        }

        Err(PciError::MsiXSetupFailed)
    }

    /// Map MSI-X structures (builder pattern)
    pub fn map_structures(mut self) -> Result<Self, PciError> {
        self.map_device_bars()?;

        let Some(bar_info) = self.device.bars.get(self.table_bar as usize) else {
            warn!("MSI-X table BAR index {} is invalid", self.table_bar);
            return Err(PciError::MsiXSetupFailed);
        };

        let super::device::BarInfo::Memory(memory_bar) = bar_info else {
            warn!("MSI-X table BAR {} is not a memory BAR", self.table_bar);
            return Err(PciError::MsiXSetupFailed);
        };

        if memory_bar.address.as_u64() == 0 {
            warn!("MSI-X table BAR {} not assigned by UEFI", self.table_bar);
            return Err(PciError::MsiXSetupFailed);
        }

        let virtual_base = self.find_bar_virtual_address(self.table_bar)?;
        self.table_virtual_addr = Some(virtual_base + self.table_offset as u64);

        info!(
            "MSI-X table mapped: phys={:#x} -> virt={:#x} (offset={:#x})",
            memory_bar.address.as_u64(),
            self.table_virtual_addr.unwrap(),
            self.table_offset
        );

        let Some(bar_info) = self.device.bars.get(self.pba_bar as usize) else {
            warn!("MSI-X PBA BAR index {} is invalid", self.pba_bar);
            return Err(PciError::MsiXSetupFailed);
        };

        let super::device::BarInfo::Memory(memory_bar) = bar_info else {
            warn!("MSI-X PBA BAR {} is not a memory BAR", self.pba_bar);
            return Err(PciError::MsiXSetupFailed);
        };

        if memory_bar.address.as_u64() == 0 {
            warn!("MSI-X PBA BAR {} not assigned by UEFI", self.pba_bar);
            return Err(PciError::MsiXSetupFailed);
        }

        let virtual_base = self.find_bar_virtual_address(self.pba_bar)?;
        self.pba_virtual_addr = Some(virtual_base + self.pba_offset as u64);

        info!(
            "MSI-X PBA mapped: phys={:#x} -> virt={:#x} (offset={:#x})",
            memory_bar.address.as_u64(),
            self.pba_virtual_addr.unwrap(),
            self.pba_offset
        );

        Ok(self)
    }

    /// Zero the PBA
    pub fn zero_pba(self) -> Result<Self, PciError> {
        if let Some(pba_addr) = self.pba_virtual_addr {
            let pba_bytes = self.table_size.div_ceil(8);
            unsafe {
                write_bytes(pba_addr as *mut u8, 0, pba_bytes as usize);
            }
        }

        Ok(self)
    }

    /// Allocate vectors
    pub fn allocate_vectors(mut self, num_vectors: u16, base_vector: u8) -> Result<Self, PciError> {
        if num_vectors > self.table_size {
            return Err(PciError::MsiXSetupFailed);
        }

        self.vectors.clear();

        for i in 0..num_vectors {
            let vector = MsiXVector {
                index: i,
                vector: base_vector + i as u8,
                enabled: false,
            };
            self.vectors.push(vector);
        }

        let Some(table_addr) = self.table_virtual_addr else {
            return Err(PciError::MsiXSetupFailed);
        };

        for vector in &self.vectors {
            let entry_addr = table_addr + (vector.index as u64 * core::mem::size_of::<MsiXTableEntry>() as u64);

            let mut entry = MsiXTableEntry::new();
            let msi_address = calculate_msi_address(0);
            let msi_data = calculate_msi_data(vector.vector);

            entry.set_address(msi_address);
            entry.set_data(msi_data);
            entry.mask();

            unsafe {
                core::ptr::write_volatile(entry_addr as *mut MsiXTableEntry, entry);
            }

            info!(
                "MSI-X vector {} allocated: vector={}, addr={:#x}",
                vector.index, vector.vector, entry_addr
            );
        }

        Ok(self)
    }

    /// Enable MSI-X for device
    pub fn enable(self) -> Result<Self, PciError> {
        let mut control = read_config_u16(
            &self.device.ecam_region,
            self.device.bus,
            self.device.device,
            self.device.function,
            self.cap_offset + msix_offsets::MESSAGE_CONTROL,
        );

        control |= msix_control_bits::MSI_X_ENABLE;

        write_config_u16(
            &self.device.ecam_region,
            self.device.bus,
            self.device.device,
            self.device.function,
            self.cap_offset + msix_offsets::MESSAGE_CONTROL,
            control,
        );

        info!(
            "MSI-X enabled for device {:02x}:{:02x}.{} with {} vectors",
            self.device.bus, self.device.device, self.device.function, self.vectors.len()
        );

        Ok(self)
    }

    /// Disable MSI-X for this device
    pub fn disable(&mut self) -> Result<(), PciError> {
        let mut control = read_config_u16(
            &self.device.ecam_region,
            self.device.bus,
            self.device.device,
            self.device.function,
            self.cap_offset + msix_offsets::MESSAGE_CONTROL,
        );

        control &= !msix_control_bits::MSI_X_ENABLE;

        write_config_u16(
            &self.device.ecam_region,
            self.device.bus,
            self.device.device,
            self.device.function,
            self.cap_offset + msix_offsets::MESSAGE_CONTROL,
            control,
        );

        Ok(())
    }

    /// Enable a specific MSI-X vector
    pub fn enable_vector(&mut self, index: u16) -> Result<(), PciError> {
        let Some(vector) = self.vectors.iter_mut().find(|v| v.index == index) else {
            return Err(PciError::InvalidDevice);
        };

        vector.enabled = true;

        let Some(table_addr) = self.table_virtual_addr else {
            return Ok(()); // Vector state updated, but no hardware table to modify
        };

        let entry_addr = table_addr + (index as u64 * core::mem::size_of::<MsiXTableEntry>() as u64);
        unsafe {
            let mut entry = core::ptr::read_volatile(entry_addr as *const MsiXTableEntry);
            entry.unmask();
            core::ptr::write_volatile(entry_addr as *mut MsiXTableEntry, entry);
        }

        Ok(())
    }

    /// Disable a specific MSI-X vector
    pub fn disable_vector(&mut self, index: u16) -> Result<(), PciError> {
        let Some(vector) = self.vectors.iter_mut().find(|v| v.index == index) else {
            return Err(PciError::InvalidDevice);
        };

        vector.enabled = false;

        let Some(table_addr) = self.table_virtual_addr else {
            return Ok(()); // Vector state updated, but no hardware table to modify
        };

        let entry_addr = table_addr + (index as u64 * core::mem::size_of::<MsiXTableEntry>() as u64);
        unsafe {
            let mut entry = core::ptr::read_volatile(entry_addr as *const MsiXTableEntry);
            entry.mask();
            core::ptr::write_volatile(entry_addr as *mut MsiXTableEntry, entry);
        }

        Ok(())
    }

    /// Mask all MSI-X vectors
    pub fn mask_all_vectors(&mut self) -> Result<(), PciError> {
        let Some(table_addr) = self.table_virtual_addr else {
            return Err(PciError::MsiXSetupFailed);
        };

        for vector in &mut self.vectors {
            vector.enabled = false;
            let entry_addr =
                table_addr + (vector.index as u64 * core::mem::size_of::<MsiXTableEntry>() as u64);
            unsafe {
                let mut entry = core::ptr::read_volatile(entry_addr as *const MsiXTableEntry);
                entry.mask();
                core::ptr::write_volatile(entry_addr as *mut MsiXTableEntry, entry);
            }
        }

        Ok(())
    }

    /// Unmask all MSI-X vectors
    pub fn unmask_all_vectors(&mut self) -> Result<(), PciError> {
        let Some(table_addr) = self.table_virtual_addr else {
            return Err(PciError::MsiXSetupFailed);
        };

        for vector in &mut self.vectors {
            vector.enabled = true;
            let entry_addr =
                table_addr + (vector.index as u64 * core::mem::size_of::<MsiXTableEntry>() as u64);
            unsafe {
                let mut entry = core::ptr::read_volatile(entry_addr as *const MsiXTableEntry);
                entry.unmask();
                core::ptr::write_volatile(entry_addr as *mut MsiXTableEntry, entry);
            }
        }

        Ok(())
    }

    /// Read the pending bit array to get all pending vectors
    /// Returns a Vec of vector indices that have pending interrupts
    pub fn read_pending_vectors(&self) -> Result<Vec<u16>, PciError> {
        let pba_addr = self.pba_virtual_addr.ok_or(PciError::MsiXSetupFailed)?;
        let mut pending_vectors = Vec::new();

        let num_vectors = self.table_size;
        let num_qwords = num_vectors.div_ceil(64); // Round up to nearest 64-bit boundary

        for qword_index in 0..num_qwords {
            let qword_addr = pba_addr + (qword_index as u64 * 8);
            let pending_bits = unsafe { core::ptr::read_volatile(qword_addr as *const u64) };

            for bit_index in 0..64 {
                let vector_index = qword_index * 64 + bit_index;
                if vector_index >= num_vectors {
                    break; 
                }

                if (pending_bits & (1u64 << bit_index)) != 0 {
                    pending_vectors.push(vector_index);
                }
            }
        }

        Ok(pending_vectors)
    }

    /// Check if a specific vector has a pending interrupt
    pub fn is_vector_pending(&self, index: u16) -> Result<bool, PciError> {
        if index >= self.table_size {
            return Err(PciError::InvalidDevice);
        }

        let pba_addr = self.pba_virtual_addr.ok_or(PciError::MsiXSetupFailed)?;

        let qword_index = index / 64;
        let bit_index = index % 64;
        let qword_addr = pba_addr + (qword_index as u64 * 8);

        let pending_bits = unsafe { core::ptr::read_volatile(qword_addr as *const u64) };
        Ok((pending_bits & (1u64 << bit_index)) != 0)
    }

    /// Get the count of pending interrupts across all vectors
    pub fn get_pending_count(&self) -> Result<u16, PciError> {
        let pba_addr = self.pba_virtual_addr.ok_or(PciError::MsiXSetupFailed)?;
        let mut count = 0;

        let num_vectors = self.table_size;
        let num_qwords = num_vectors.div_ceil(64);

        for qword_index in 0..num_qwords {
            let qword_addr = pba_addr + (qword_index as u64 * 8);
            let pending_bits = unsafe { core::ptr::read_volatile(qword_addr as *const u64) };

            // Count bits in this qword, but don't count beyond our actual vector count
            let vectors_in_this_qword = core::cmp::min(64, num_vectors - qword_index * 64);
            let mask = if vectors_in_this_qword == 64 {
                u64::MAX
            } else {
                (1u64 << vectors_in_this_qword) - 1
            };

            count += (pending_bits & mask).count_ones() as u16;
        }

        Ok(count)
    }

    /// Check if any vectors have pending interrupts
    pub fn has_pending_interrupts(&self) -> Result<bool, PciError> {
        Ok(self.get_pending_count()? > 0)
    }
}

/// Calculate MSI address for x86-64 Local APIC
fn calculate_msi_address(cpu_id: u8) -> u64 {
    // MSI address format for x86-64:
    // Bits 31-20: 0xFEE (fixed)
    // Bits 19-12: Destination ID (APIC ID)
    // Bits 11-4: Reserved (0)
    // Bits 3: Redirection Hint (0 = directed, 1 = lowest priority)
    // Bits 2: Destination Mode (0 = physical, 1 = logical)
    // Bits 1-0: Reserved (00)

    0xFEE00000 | ((cpu_id as u64) << 12)
}

/// Calculate MSI data for interrupt vector
fn calculate_msi_data(vector: u8) -> u32 {
    // MSI data format for x86-64:
    // Bits 31-16: Reserved (0)
    // Bits 15: Trigger Mode (0 = edge, 1 = level)
    // Bits 14: Level (0 = deassert, 1 = assert) - only for level triggered
    // Bits 13-11: Reserved (000)
    // Bits 10-8: Delivery Mode (000 = fixed, 001 = lowest priority, etc.)
    // Bits 7-0: Vector

    vector as u32 // Edge-triggered, fixed delivery mode
}

/// Setup MSI-X for a device
pub fn setup_msix(
    device: &PciDevice,
    num_vectors: u16,
    base_vector: u8,
) -> Result<MsiXInfo, PciError> {
    let cap = device
        .find_capability(capability_ids::MSI_X)
        .ok_or(PciError::MsiXSetupFailed)?;

    MsiXInfo::from_device(device, cap as u16)?
        .map_structures()?
        .zero_pba()?
        .allocate_vectors(num_vectors, base_vector)?
        .enable()
}
