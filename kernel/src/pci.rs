//! PCIe (Peripheral Component Interconnect Express) support for the kernel.
//!
//! This module provides comprehensive PCIe device discovery, configuration,
//! and interrupt handling capabilities including:
//!
//! - ACPI MCFG table parsing for Enhanced Configuration Access Mechanism (ECAM)
//! - PCIe configuration space access via memory-mapped I/O
//! - Device enumeration and capability discovery
//! - MSI-X interrupt setup and management
//! - Device driver interface and registration

pub mod config;
pub mod device;
pub mod mcfg;
pub mod msi;

use alloc::vec::Vec;
use spin::Mutex;

use crate::info;

/// Global PCIe manager instance
pub static PCI_MANAGER: Mutex<Option<PciManager>> = Mutex::new(None);

/// Main PCIe management structure
pub struct PciManager {
    /// List of discovered PCIe devices
    pub devices: Vec<device::PciDevice>,
    /// ECAM (Enhanced Configuration Access Mechanism) regions
    pub ecam_regions: Vec<mcfg::EcamRegion>,
}

impl Default for PciManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PciManager {
    /// Create a new PCIe manager
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            ecam_regions: Vec::new(),
        }
    }

    /// Initialize the PCIe subsystem
    pub fn init(&mut self, rsdp_addr: usize) -> Result<(), PciError> {
        info!("Initializing PCIe subsystem");
        
        // Parse MCFG table to get ECAM regions
        self.ecam_regions = mcfg::parse_mcfg_table(rsdp_addr)?;
        info!("Found {} ECAM regions", self.ecam_regions.len());

        // Calculate total memory that will be mapped
        let total_size = mcfg::calculate_total_ecam_size(&self.ecam_regions);
        info!("Total ECAM mapping size: {} MB", total_size >> 20);

        // Map entire ECAM regions to virtual memory
        for region in &mut self.ecam_regions {
            mcfg::map_ecam_region(region)?;
        }

        info!("All ECAM regions mapped successfully");

        // Enumerate all PCIe devices
        self.enumerate_devices()?;
        info!("Discovered {} PCIe devices", self.devices.len());

        Ok(())
    }

    /// Enumerate all PCIe devices across all buses
    fn enumerate_devices(&mut self) -> Result<(), PciError> {
        // Clone the regions to avoid borrowing issues
        let regions = self.ecam_regions.clone();
        for ecam_region in &regions {
            for bus in ecam_region.start_bus..=ecam_region.end_bus {
                self.enumerate_bus(ecam_region, bus)?;
            }
        }
        Ok(())
    }

    /// Enumerate devices on a specific bus
    fn enumerate_bus(&mut self, ecam_region: &mcfg::EcamRegion, bus: u8) -> Result<(), PciError> {
        for device in 0..32 {
            for function in 0..8 {
                if let Some(pci_device) = device::probe_device(ecam_region, bus, device, function)? {
                    self.devices.push(pci_device);
                    
                    // If this is function 0 and not a multi-function device, skip other functions
                    if function == 0 && !device::is_multifunction_device(ecam_region, bus, device, 0)? {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Find a device by vendor and device ID
    pub fn find_device(&self, vendor_id: u16, device_id: u16) -> Option<&device::PciDevice> {
        self.devices.iter().find(|dev| {
            dev.vendor_id == vendor_id && dev.device_id == device_id
        })
    }

    /// Get all devices of a specific class
    pub fn get_devices_by_class(&self, class_code: u8) -> Vec<&device::PciDevice> {
        self.devices.iter().filter(|dev| dev.class_code == class_code).collect()
    }
}

/// PCIe-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PciError {
    /// MCFG table not found or invalid
    McfgNotFound,
    /// Failed to map ECAM region
    EcamMappingFailed,
    /// Invalid device configuration
    InvalidDevice,
    /// MSI-X setup failed
    MsiXSetupFailed,
    /// Memory allocation failed
    AllocationFailed,
}

/// Initialize the global PCIe manager
pub fn init_pci(rsdp_addr: usize) -> Result<(), PciError> {
    let mut manager = PciManager::new();
    manager.init(rsdp_addr)?;
    
    let mut pci_lock = PCI_MANAGER.lock();
    *pci_lock = Some(manager);
    
    info!("PCIe subsystem initialized successfully");
    Ok(())
}

/// Get a reference to the global PCIe manager
pub fn get_pci_manager() -> &'static Mutex<Option<PciManager>> {
    &PCI_MANAGER
}
