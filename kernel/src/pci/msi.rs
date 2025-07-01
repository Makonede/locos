//! MSI and MSI-X interrupt handling for PCIe devices.
//!
//! This module provides:
//! - MSI (Message Signaled Interrupts) setup and management
//! - MSI-X (Extended Message Signaled Interrupts) setup and management
//! - Interrupt vector allocation and routing
//! - Device interrupt configuration

use alloc::vec::Vec;

use crate::{
    info, warn,
};

use super::{
    device::PciDevice,
    mcfg::{read_config_u16, read_config_u32, write_config_u16, write_config_u32},
    config::{
        capability_ids, msi_offsets, msix_offsets, msi_control_bits, msix_control_bits,
        MsiXTableEntry,
    },
    PciError,
};

/// MSI-X virtual address space start
#[allow(dead_code)]
const MSIX_VIRTUAL_START: u64 = 0xFFFF_F500_0000_0000;

/// MSI interrupt information
#[derive(Debug, Clone)]
pub struct MsiInfo {
    /// Device that owns this MSI
    pub device: PciDevice,
    /// Capability offset in configuration space
    pub cap_offset: u16,
    /// Number of vectors supported
    pub vectors_supported: u8,
    /// Number of vectors allocated
    pub vectors_allocated: u8,
    /// Base interrupt vector number
    pub base_vector: u8,
    /// Whether 64-bit addressing is supported
    pub is_64bit: bool,
    /// Whether per-vector masking is supported
    pub per_vector_masking: bool,
}

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

impl MsiInfo {
    /// Create MSI information from a device capability
    pub fn from_device(device: &PciDevice, cap_offset: u16) -> Result<Self, PciError> {
        let control = read_config_u16(&device.ecam_region, device.bus, device.device, device.function, cap_offset + msi_offsets::MESSAGE_CONTROL);
        
        let vectors_supported = 1 << ((control & msi_control_bits::MULTIPLE_MESSAGE_CAPABLE_MASK) >> 1);
        let is_64bit = (control & msi_control_bits::ADDRESS_64_CAPABLE) != 0;
        let per_vector_masking = (control & msi_control_bits::PER_VECTOR_MASKING_CAPABLE) != 0;

        Ok(Self {
            device: device.clone(),
            cap_offset,
            vectors_supported,
            vectors_allocated: 0,
            base_vector: 0,
            is_64bit,
            per_vector_masking,
        })
    }

    /// Enable MSI for this device
    pub fn enable(&mut self, base_vector: u8, num_vectors: u8) -> Result<(), PciError> {
        if num_vectors > self.vectors_supported {
            return Err(PciError::MsiXSetupFailed);
        }

        self.base_vector = base_vector;
        self.vectors_allocated = num_vectors;

        // Calculate MSI address and data
        let msi_address = calculate_msi_address(0); // CPU 0 for now
        let msi_data = calculate_msi_data(base_vector);

        // Write MSI address
        write_config_u32(
            &self.device.ecam_region,
            self.device.bus,
            self.device.device,
            self.device.function,
            self.cap_offset + msi_offsets::MESSAGE_ADDRESS_LOW,
            msi_address as u32,
        );

        if self.is_64bit {
            write_config_u32(
                &self.device.ecam_region,
                self.device.bus,
                self.device.device,
                self.device.function,
                self.cap_offset + msi_offsets::MESSAGE_ADDRESS_HIGH,
                (msi_address >> 32) as u32,
            );
            
            write_config_u32(
                &self.device.ecam_region,
                self.device.bus,
                self.device.device,
                self.device.function,
                self.cap_offset + msi_offsets::MESSAGE_DATA_64,
                msi_data,
            );
        } else {
            write_config_u32(
                &self.device.ecam_region,
                self.device.bus,
                self.device.device,
                self.device.function,
                self.cap_offset + msi_offsets::MESSAGE_DATA_32,
                msi_data,
            );
        }

        // Configure number of vectors and enable MSI
        let mut control = read_config_u16(
            &self.device.ecam_region,
            self.device.bus,
            self.device.device,
            self.device.function,
            self.cap_offset + msi_offsets::MESSAGE_CONTROL,
        );

        // Set number of enabled vectors
        control &= !msi_control_bits::MULTIPLE_MESSAGE_ENABLE_MASK;
        control |= ((num_vectors.trailing_zeros() as u16) << 4) & msi_control_bits::MULTIPLE_MESSAGE_ENABLE_MASK;
        
        // Enable MSI
        control |= msi_control_bits::MSI_ENABLE;

        write_config_u16(
            &self.device.ecam_region,
            self.device.bus,
            self.device.device,
            self.device.function,
            self.cap_offset + msi_offsets::MESSAGE_CONTROL,
            control,
        );

        info!(
            "Enabled MSI for device {:02x}:{:02x}.{}: {} vectors starting at {}",
            self.device.bus, self.device.device, self.device.function,
            num_vectors, base_vector
        );

        Ok(())
    }

    /// Disable MSI for this device
    pub fn disable(&mut self) -> Result<(), PciError> {
        let mut control = read_config_u16(
            &self.device.ecam_region,
            self.device.bus,
            self.device.device,
            self.device.function,
            self.cap_offset + msi_offsets::MESSAGE_CONTROL,
        );

        control &= !msi_control_bits::MSI_ENABLE;

        write_config_u16(
            &self.device.ecam_region,
            self.device.bus,
            self.device.device,
            self.device.function,
            self.cap_offset + msi_offsets::MESSAGE_CONTROL,
            control,
        );

        self.vectors_allocated = 0;
        self.base_vector = 0;

        Ok(())
    }
}

impl MsiXInfo {
    /// Create MSI-X information from a device capability
    pub fn from_device(device: &PciDevice, cap_offset: u16) -> Result<Self, PciError> {
        let control = read_config_u16(&device.ecam_region, device.bus, device.device, device.function, cap_offset + msix_offsets::MESSAGE_CONTROL);
        let table_offset_bir = read_config_u32(&device.ecam_region, device.bus, device.device, device.function, cap_offset + msix_offsets::TABLE_OFFSET_BIR);
        let pba_offset_bir = read_config_u32(&device.ecam_region, device.bus, device.device, device.function, cap_offset + msix_offsets::PBA_OFFSET_BIR);

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
        })
    }

    /// Map MSI-X table and PBA to virtual memory
    pub fn map_structures(&mut self) -> Result<(), PciError> {
        use super::device::BarInfo;

        // Get the BAR that contains the MSI-X table
        if let Some(table_bar_info) = self.device.bars.get(self.table_bar as usize) {
            match table_bar_info {
                BarInfo::Memory { address, .. } => {
                    self.table_virtual_addr = Some(address.as_u64() + self.table_offset as u64);
                    info!(
                        "MSI-X table mapped at virtual address {:#x}",
                        self.table_virtual_addr.unwrap()
                    );
                }
                _ => {
                    warn!("MSI-X table BAR is not a memory BAR");
                    return Err(PciError::MsiXSetupFailed);
                }
            }
        } else {
            warn!("MSI-X table BAR index {} is invalid", self.table_bar);
            return Err(PciError::MsiXSetupFailed);
        }

        // Get the BAR that contains the PBA
        if let Some(pba_bar_info) = self.device.bars.get(self.pba_bar as usize) {
            match pba_bar_info {
                BarInfo::Memory { address, .. } => {
                    self.pba_virtual_addr = Some(address.as_u64() + self.pba_offset as u64);
                    info!(
                        "MSI-X PBA mapped at virtual address {:#x}",
                        self.pba_virtual_addr.unwrap()
                    );
                }
                _ => {
                    warn!("MSI-X PBA BAR is not a memory BAR");
                    return Err(PciError::MsiXSetupFailed);
                }
            }
        } else {
            warn!("MSI-X PBA BAR index {} is invalid", self.pba_bar);
            return Err(PciError::MsiXSetupFailed);
        }

        Ok(())
    }

    /// Allocate and configure MSI-X vectors
    pub fn allocate_vectors(&mut self, num_vectors: u16, base_vector: u8) -> Result<(), PciError> {
        if num_vectors > self.table_size {
            return Err(PciError::MsiXSetupFailed);
        }

        // Clear existing vectors
        self.vectors.clear();

        // Allocate new vectors
        for i in 0..num_vectors {
            let vector = MsiXVector {
                index: i,
                vector: base_vector + i as u8,
                enabled: false,
            };
            self.vectors.push(vector);
        }

        // Configure each vector in the table
        if let Some(table_addr) = self.table_virtual_addr {
            for vector in &self.vectors {
                let entry_addr = table_addr + (vector.index as u64 * core::mem::size_of::<MsiXTableEntry>() as u64);
                let mut entry = MsiXTableEntry::new();
                
                let msi_address = calculate_msi_address(0); // CPU 0 for now
                let msi_data = calculate_msi_data(vector.vector);
                
                entry.set_address(msi_address);
                entry.set_data(msi_data);
                entry.mask(); // Start masked
                
                unsafe {
                    core::ptr::write_volatile(entry_addr as *mut MsiXTableEntry, entry);
                }
            }
        }

        info!(
            "Allocated {} MSI-X vectors for device {:02x}:{:02x}.{}",
            num_vectors, self.device.bus, self.device.device, self.device.function
        );

        Ok(())
    }

    /// Enable MSI-X for this device
    pub fn enable(&mut self) -> Result<(), PciError> {
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
            "Enabled MSI-X for device {:02x}:{:02x}.{}",
            self.device.bus, self.device.device, self.device.function
        );

        Ok(())
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
        if let Some(vector) = self.vectors.iter_mut().find(|v| v.index == index) {
            vector.enabled = true;
            
            if let Some(table_addr) = self.table_virtual_addr {
                let entry_addr = table_addr + (index as u64 * core::mem::size_of::<MsiXTableEntry>() as u64);
                unsafe {
                    let mut entry = core::ptr::read_volatile(entry_addr as *const MsiXTableEntry);
                    entry.unmask();
                    core::ptr::write_volatile(entry_addr as *mut MsiXTableEntry, entry);
                }
            }
            
            Ok(())
        } else {
            Err(PciError::InvalidDevice)
        }
    }

    /// Disable a specific MSI-X vector
    pub fn disable_vector(&mut self, index: u16) -> Result<(), PciError> {
        if let Some(vector) = self.vectors.iter_mut().find(|v| v.index == index) {
            vector.enabled = false;
            
            if let Some(table_addr) = self.table_virtual_addr {
                let entry_addr = table_addr + (index as u64 * core::mem::size_of::<MsiXTableEntry>() as u64);
                unsafe {
                    let mut entry = core::ptr::read_volatile(entry_addr as *const MsiXTableEntry);
                    entry.mask();
                    core::ptr::write_volatile(entry_addr as *mut MsiXTableEntry, entry);
                }
            }
            
            Ok(())
        } else {
            Err(PciError::InvalidDevice)
        }
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

/// Setup MSI for a device
pub fn setup_msi(device: &PciDevice, num_vectors: u8, base_vector: u8) -> Result<MsiInfo, PciError> {
    let cap = device.find_capability(capability_ids::MSI)
        .ok_or(PciError::MsiXSetupFailed)?;
    
    let mut msi_info = MsiInfo::from_device(device, cap.next_ptr as u16)?;
    msi_info.enable(base_vector, num_vectors)?;
    
    Ok(msi_info)
}

/// Setup MSI-X for a device
pub fn setup_msix(device: &PciDevice, num_vectors: u16, base_vector: u8) -> Result<MsiXInfo, PciError> {
    let cap = device.find_capability(capability_ids::MSI_X)
        .ok_or(PciError::MsiXSetupFailed)?;
    
    let mut msix_info = MsiXInfo::from_device(device, cap.next_ptr as u16)?;
    msix_info.map_structures()?;
    msix_info.allocate_vectors(num_vectors, base_vector)?;
    msix_info.enable()?;
    
    Ok(msix_info)
}
