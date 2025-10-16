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
pub mod vmm;

pub mod usb;
pub mod nvme;

pub use usb::init;

#[cfg(test)]
pub mod tests;

use alloc::vec::Vec;
use spin::Mutex;

use crate::{
    info,
    pci::{
        device::{IoBar, MemoryBar},
        vmm::PCIE_VMM,
        msi::MsiXInfo,
    },
    warn,
};

/// Global PCIe manager instance
pub static PCI_MANAGER: Mutex<Option<PciManager>> = Mutex::new(None);

/// Main PCIe management structure
pub struct PciManager {
    /// List of discovered PCIe devices
    pub devices: Vec<device::PciDevice>,
    /// ECAM (Enhanced Configuration Access Mechanism) regions
    pub ecam_regions: Vec<mcfg::EcamRegion>,
    /// MSI-X configurations for devices that support it
    pub msix_devices: Vec<MsiXInfo>,
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
            msix_devices: Vec::new(),
        }
    }

    /// Initialize the PCIe subsystem
    pub fn init(&mut self, rsdp_addr: usize) -> Result<(), PciError> {
        info!("Initializing PCIe subsystem");

        self.ecam_regions = mcfg::parse_mcfg_table(rsdp_addr)?;
        info!("Found {} ECAM regions", self.ecam_regions.len());

        let total_size = mcfg::calculate_total_ecam_size(&self.ecam_regions);
        info!("Total ECAM mapping size: {} MB", total_size >> 20);

        for region in &mut self.ecam_regions {
            mcfg::map_ecam_region(region)?;
        }

        info!("All ECAM regions mapped successfully");

        self.enumerate_devices()?;
        info!("Discovered {} PCIe devices", self.devices.len());

        self.check_bar_assignment();

        self.msix_devices = msi::init_msix_devices(&self.devices)?;

        Ok(())
    }

    /// Enumerate all PCIe devices across all buses
    fn enumerate_devices(&mut self) -> Result<(), PciError> {
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
                if let Some(pci_device) = device::probe_device(ecam_region, bus, device, function)?
                {
                    self.devices.push(pci_device);

                    // If this is function 0 and not a multi-function device, skip other functions
                    if function == 0
                        && !device::is_multifunction_device(ecam_region, bus, device, 0)?
                    {
                        break;
                    }
                }
            }
        }
        Ok(())
    }



    /// Find a device by vendor and device ID
    pub fn find_device(&self, vendor_id: u16, device_id: u16) -> Option<&device::PciDevice> {
        self.devices
            .iter()
            .find(|dev| dev.vendor_id == vendor_id && dev.device_id == device_id)
    }

    /// Get all devices of a specific class
    pub fn get_devices_by_class(&self, class_code: u8) -> Vec<&device::PciDevice> {
        self.devices
            .iter()
            .filter(|dev| dev.class_code == class_code)
            .collect()
    }

    /// Get all MSI-X configured devices
    pub fn get_msix_devices(&self) -> &Vec<MsiXInfo> {
        &self.msix_devices
    }

    /// Find MSI-X info for a specific device
    pub fn find_msix_device(&self, bus: u8, device: u8, function: u8) -> Option<&MsiXInfo> {
        self.msix_devices.iter().find(|msix| {
            msix.device.bus == bus
                && msix.device.device == device
                && msix.device.function == function
        })
    }

    /// Check BAR assignment status for all devices
    fn check_bar_assignment(&self) {
        let mut assigned_count = 0;
        let mut unassigned_count = 0;

        for device in &self.devices {
            for (i, bar) in device.bars.iter().enumerate() {
                match bar {
                    device::BarInfo::Memory(MemoryBar { address, size, .. }) => {
                        if address.as_u64() == 0 {
                            warn!(
                                "Device {:02x}:{:02x}.{} BAR{}: Memory BAR not assigned by UEFI (size={}KB)",
                                device.bus,
                                device.device,
                                device.function,
                                i,
                                size >> 10
                            );
                            unassigned_count += 1;
                        } else if *size == 0 {
                            warn!(
                                "Device {:02x}:{:02x}.{} BAR{}: Memory BAR has zero size at {:#x}",
                                device.bus,
                                device.device,
                                device.function,
                                i,
                                address.as_u64()
                            );
                        } else {
                            assigned_count += 1;
                        }
                    }
                    device::BarInfo::Io(IoBar { address, size }) => {
                        if *address == 0 {
                            warn!(
                                "Device {:02x}:{:02x}.{} BAR{}: I/O BAR not assigned by UEFI (size={}B)",
                                device.bus, device.device, device.function, i, size
                            );
                            unassigned_count += 1;
                        } else {
                            assigned_count += 1;
                        }
                    }
                    device::BarInfo::Unused => {}
                }
            }
        }

        info!(
            "BAR assignment check: {} assigned, {} unassigned",
            assigned_count, unassigned_count
        );

        if unassigned_count > 0 {
            warn!(
                "{} BARs were not assigned addresses by UEFI!",
                unassigned_count
            );
        }

        // Print VMM statistics
        let vmm_lock = PCIE_VMM.lock();
        let stats = vmm_lock.get_stats();
        info!(
            "PCIe VMM initialized: {}/{} pages available ({}MB/{}MB)",
            stats.free_pages,
            stats.total_pages,
            stats.free_size >> 20,
            stats.total_size >> 20
        );
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
