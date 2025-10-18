//! PCIe device representation and enumeration.
//!
//! This module provides:
//! - PCIe device structure and identification
//! - Device probing and capability discovery
//! - Base Address Register (BAR) parsing
//! - Device class and vendor identification

use alloc::collections::BTreeMap;
use core::fmt;
use x86_64::PhysAddr;

use crate::debug;

use super::{
    PciError,
    mcfg::{EcamRegion, read_config_u8, read_config_u16, read_config_u32},
};

/// PCIe configuration space offsets
pub mod config_offsets {
    pub const VENDOR_ID: u16 = 0x00;
    pub const DEVICE_ID: u16 = 0x02;
    pub const COMMAND: u16 = 0x04;
    pub const STATUS: u16 = 0x06;
    pub const REVISION_ID: u16 = 0x08;
    pub const PROG_IF: u16 = 0x09;
    pub const SUBCLASS: u16 = 0x0A;
    pub const CLASS_CODE: u16 = 0x0B;
    pub const CACHE_LINE_SIZE: u16 = 0x0C;
    pub const LATENCY_TIMER: u16 = 0x0D;
    pub const HEADER_TYPE: u16 = 0x0E;
    pub const BIST: u16 = 0x0F;
    pub const BAR0: u16 = 0x10;
    pub const BAR1: u16 = 0x14;
    pub const BAR2: u16 = 0x18;
    pub const BAR3: u16 = 0x1C;
    pub const BAR4: u16 = 0x20;
    pub const BAR5: u16 = 0x24;
    pub const CARDBUS_CIS: u16 = 0x28;
    pub const SUBSYSTEM_VENDOR_ID: u16 = 0x2C;
    pub const SUBSYSTEM_ID: u16 = 0x2E;
    pub const EXPANSION_ROM: u16 = 0x30;
    pub const CAPABILITIES_PTR: u16 = 0x34;
    pub const INTERRUPT_LINE: u16 = 0x3C;
    pub const INTERRUPT_PIN: u16 = 0x3D;
    pub const MIN_GRANT: u16 = 0x3E;
    pub const MAX_LATENCY: u16 = 0x3F;
}

/// PCIe device header types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderType {
    Normal = 0x00,
    PciToPciBridge = 0x01,
    CardBusBridge = 0x02,
}

/// Base Address Register (BAR) information
#[derive(Debug, Clone, Copy)]
pub enum BarInfo {
    /// Memory BAR (physical address)
    Memory(MemoryBar),
    /// I/O BAR (I/O port address)
    Io(IoBar),
    /// Unused BAR
    Unused,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryBar {
    pub address: PhysAddr,
    pub size: u64,
    pub prefetchable: bool,
    pub is_64bit: bool,
}

impl MemoryBar {
    pub fn new(address: PhysAddr, size: u64, prefetchable: bool, is_64bit: bool) -> Self {
        Self {
            address,
            size,
            prefetchable,
            is_64bit,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IoBar {
    pub address: u32,
    pub size: u32,
}

impl IoBar {
    pub fn new(address: u32, size: u32) -> Self {
        Self { address, size }
    }
}

/// PCIe device representation
#[derive(Debug, Clone)]
pub struct PciDevice {
    /// ECAM region this device belongs to
    pub ecam_region: EcamRegion,
    /// Bus number
    pub bus: u8,
    /// Device number
    pub device: u8,
    /// Function number
    pub function: u8,
    /// Vendor ID
    pub vendor_id: u16,
    /// Device ID
    pub device_id: u16,
    /// Device class code
    pub class_code: u8,
    /// Device subclass
    pub subclass: u8,
    /// Programming interface
    pub prog_if: u8,
    /// Revision ID
    pub revision_id: u8,
    /// Header type
    pub header_type: HeaderType,
    /// Subsystem vendor ID
    pub subsystem_vendor_id: u16,
    /// Subsystem ID
    pub subsystem_id: u16,
    /// Base Address Registers
    pub bars: [BarInfo; 6],
    /// Map of capability ID to capability offset
    pub capabilities: BTreeMap<u8, u8>,
    /// Interrupt line
    pub interrupt_line: u8,
    /// Interrupt pin
    pub interrupt_pin: u8,
}

impl PciDevice {
    /// Get a human-readable device description
    pub fn description(&self) -> &'static str {
        match (self.class_code, self.subclass) {
            (0x00, 0x00) => "Legacy Device",
            (0x01, 0x00) => "SCSI Bus Controller",
            (0x01, 0x01) => "IDE Controller",
            (0x01, 0x02) => "Floppy Disk Controller",
            (0x01, 0x03) => "IPI Bus Controller",
            (0x01, 0x04) => "RAID Controller",
            (0x01, 0x05) => "ATA Controller",
            (0x01, 0x06) => "SATA Controller",
            (0x01, 0x07) => "SAS Controller",
            (0x01, 0x08) => "NVM Controller",
            (0x02, 0x00) => "Ethernet Controller",
            (0x02, 0x01) => "Token Ring Controller",
            (0x02, 0x02) => "FDDI Controller",
            (0x02, 0x03) => "ATM Controller",
            (0x02, 0x04) => "ISDN Controller",
            (0x02, 0x05) => "WorldFip Controller",
            (0x02, 0x06) => "PICMG 2.14 Multi Computing",
            (0x02, 0x07) => "Infiniband Controller",
            (0x02, 0x08) => "Fabric Controller",
            (0x03, 0x00) => "VGA Compatible Controller",
            (0x03, 0x01) => "XGA Controller",
            (0x03, 0x02) => "3D Controller",
            (0x04, 0x00) => "Multimedia Video Controller",
            (0x04, 0x01) => "Multimedia Audio Controller",
            (0x04, 0x02) => "Computer Telephony Device",
            (0x04, 0x03) => "Audio Device",
            (0x05, 0x00) => "RAM Controller",
            (0x05, 0x01) => "Flash Controller",
            (0x06, 0x00) => "Host Bridge",
            (0x06, 0x01) => "ISA Bridge",
            (0x06, 0x02) => "EISA Bridge",
            (0x06, 0x03) => "MCA Bridge",
            (0x06, 0x04) => "PCI-to-PCI Bridge",
            (0x06, 0x05) => "PCMCIA Bridge",
            (0x06, 0x06) => "NuBus Bridge",
            (0x06, 0x07) => "CardBus Bridge",
            (0x06, 0x08) => "RACEway Bridge",
            (0x06, 0x09) => "PCI-to-PCI Bridge",
            (0x06, 0x0A) => "InfiniBand-to-PCI Host Bridge",
            (0x0C, 0x00) => "FireWire Controller",
            (0x0C, 0x01) => "ACCESS Bus Controller",
            (0x0C, 0x02) => "SSA Controller",
            (0x0C, 0x03) => "USB Controller",
            (0x0C, 0x04) => "Fibre Channel Controller",
            (0x0C, 0x05) => "SMBus Controller",
            (0x0C, 0x06) => "InfiniBand Controller",
            (0x0C, 0x07) => "IPMI Interface",
            (0x0C, 0x08) => "SERCOS Interface",
            (0x0C, 0x09) => "CANbus Controller",
            _ => "Unknown Device",
        }
    }

    /// Check if device supports MSI-X
    pub fn supports_msix(&self) -> bool {
        self.capabilities.contains_key(&0x11)
    }

    /// Check if device supports MSI
    pub fn supports_msi(&self) -> bool {
        self.capabilities.contains_key(&0x05)
    }

    /// Find a capability by ID, returns the offset if found
    pub fn find_capability(&self, cap_id: u8) -> Option<u8> {
        self.capabilities.get(&cap_id).copied()
    }
}

impl fmt::Display for PciDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}.{} [{:04x}:{:04x}] {} (rev {:02x})",
            self.bus,
            self.device,
            self.function,
            self.vendor_id,
            self.device_id,
            self.description(),
            self.revision_id
        )
    }
}

/// Probe a specific PCIe device location
pub fn probe_device(
    ecam_region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
) -> Result<Option<PciDevice>, PciError> {
    // Read vendor ID to check if device exists
    let vendor_id = read_config_u16(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::VENDOR_ID,
    );

    // 0xFFFF indicates no device present
    if vendor_id == 0xFFFF {
        return Ok(None);
    }

    // Read basic device information
    let device_id = read_config_u16(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::DEVICE_ID,
    );
    let class_code = read_config_u8(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::CLASS_CODE,
    );
    let subclass = read_config_u8(ecam_region, bus, device, function, config_offsets::SUBCLASS);
    let prog_if = read_config_u8(ecam_region, bus, device, function, config_offsets::PROG_IF);
    let revision_id = read_config_u8(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::REVISION_ID,
    );
    let header_type_raw = read_config_u8(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::HEADER_TYPE,
    ) & 0x7F;
    let subsystem_vendor_id = read_config_u16(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::SUBSYSTEM_VENDOR_ID,
    );
    let subsystem_id = read_config_u16(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::SUBSYSTEM_ID,
    );
    let interrupt_line = read_config_u8(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::INTERRUPT_LINE,
    );
    let interrupt_pin = read_config_u8(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::INTERRUPT_PIN,
    );

    let header_type = match header_type_raw {
        0x00 => HeaderType::Normal,
        0x01 => HeaderType::PciToPciBridge,
        0x02 => HeaderType::CardBusBridge,
        _ => return Err(PciError::InvalidDevice),
    };

    // Parse BARs (only for normal devices)
    let bars = if header_type == HeaderType::Normal {
        parse_bars(ecam_region, bus, device, function)?
    } else {
        [BarInfo::Unused; 6]
    };

    // Parse capabilities
    let capabilities = parse_capabilities(ecam_region, bus, device, function)?;

    debug!(
        "Found PCIe device: {:02x}:{:02x}.{} [{:04x}:{:04x}] class={:02x}:{:02x}",
        bus, device, function, vendor_id, device_id, class_code, subclass
    );

    Ok(Some(PciDevice {
        ecam_region: *ecam_region,
        bus,
        device,
        function,
        vendor_id,
        device_id,
        class_code,
        subclass,
        prog_if,
        revision_id,
        header_type,
        subsystem_vendor_id,
        subsystem_id,
        bars,
        capabilities,
        interrupt_line,
        interrupt_pin,
    }))
}

/// Check if a device is multi-function
pub fn is_multifunction_device(
    ecam_region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
) -> Result<bool, PciError> {
    let header_type = read_config_u8(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::HEADER_TYPE,
    );
    Ok((header_type & 0x80) != 0)
}

/// Parse Base Address Registers for a device
fn parse_bars(
    ecam_region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
) -> Result<[BarInfo; 6], PciError> {
    let mut bars = [BarInfo::Unused; 6];
    let mut i = 0;

    while i < 6 {
        let bar_offset = config_offsets::BAR0 + (i as u16 * 4);
        let bar_value = read_config_u32(ecam_region, bus, device, function, bar_offset);

        if bar_value == 0 {
            i += 1;
            continue;
        }

        if (bar_value & 1) == 0 {
            // Memory BAR
            let is_64bit = (bar_value & 0x6) == 0x4;
            let prefetchable = (bar_value & 0x8) != 0;

            let address_raw = if is_64bit && i < 5 {
                let high_bar = read_config_u32(ecam_region, bus, device, function, bar_offset + 4);
                ((high_bar as u64) << 32) | (bar_value & 0xFFFFFFF0) as u64
            } else {
                (bar_value & 0xFFFFFFF0) as u64
            };

            // Determine BAR size using the standard method
            let size = determine_bar_size(ecam_region, bus, device, function, bar_offset, is_64bit);

            bars[i] = BarInfo::Memory(MemoryBar::new(
                PhysAddr::new(address_raw),
                size,
                prefetchable,
                is_64bit,
            ));

            if is_64bit {
                i += 2; // Skip next BAR as it's the high part
            } else {
                i += 1;
            }
        } else {
            // I/O BAR
            let address = bar_value & 0xFFFFFFFC;
            let size = determine_io_bar_size(ecam_region, bus, device, function, bar_offset);

            bars[i] = BarInfo::Io(IoBar::new(address, size));
            i += 1;
        }
    }

    Ok(bars)
}

/// Parse device capabilities
fn parse_capabilities(
    ecam_region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
) -> Result<BTreeMap<u8, u8>, PciError> {
    let mut capabilities = BTreeMap::new();

    // Check if device has capabilities
    let status = read_config_u16(ecam_region, bus, device, function, config_offsets::STATUS);
    if (status & 0x10) == 0 {
        return Ok(capabilities); // No capabilities
    }

    let mut cap_ptr = read_config_u8(
        ecam_region,
        bus,
        device,
        function,
        config_offsets::CAPABILITIES_PTR,
    );

    while cap_ptr != 0 && cap_ptr != 0xFF {
        let cap_id = read_config_u8(ecam_region, bus, device, function, cap_ptr as u16);
        let next_ptr = read_config_u8(ecam_region, bus, device, function, cap_ptr as u16 + 1);

        capabilities.insert(cap_id, cap_ptr);

        cap_ptr = next_ptr;
    }

    Ok(capabilities)
}

/// Determine the size of a memory BAR using the standard write-all-1s method
fn determine_bar_size(
    ecam_region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
    bar_offset: u16,
    is_64bit: bool,
) -> u64 {
    use super::mcfg::{read_config_u32, write_config_u32};

    // Save original BAR values
    let original_low = read_config_u32(ecam_region, bus, device, function, bar_offset);
    let original_high = if is_64bit {
        read_config_u32(ecam_region, bus, device, function, bar_offset + 4)
    } else {
        0
    };

    if is_64bit {
        write_config_u32(ecam_region, bus, device, function, bar_offset, 0xFFFFFFFF);
        write_config_u32(
            ecam_region,
            bus,
            device,
            function,
            bar_offset + 4,
            0xFFFFFFFF,
        );

        let size_low = read_config_u32(ecam_region, bus, device, function, bar_offset);
        let size_high = read_config_u32(ecam_region, bus, device, function, bar_offset + 4);

        write_config_u32(
            ecam_region,
            bus,
            device,
            function,
            bar_offset + 4,
            original_high,
        );
        write_config_u32(ecam_region, bus, device, function, bar_offset, original_low);

        let size_mask_64 = ((size_high as u64) << 32) | ((size_low & 0xFFFFFFF0) as u64);

        if size_mask_64 == 0 {
            0
        } else {
            (!size_mask_64) + 1
        }
    } else {
        write_config_u32(ecam_region, bus, device, function, bar_offset, 0xFFFFFFFF);
        let size_low = read_config_u32(ecam_region, bus, device, function, bar_offset);

        write_config_u32(ecam_region, bus, device, function, bar_offset, original_low);

        // IMPORTANT FIX: Perform bitwise NOT on u32, then cast to u64
        let size_mask_32 = size_low & 0xFFFFFFF0; // Keep as u32

        if size_mask_32 == 0 {
            0
        } else {
            !size_mask_32 as u64 + 1
        }
    }
}

/// Determine the size of an I/O BAR using the standard write-all-1s method
fn determine_io_bar_size(
    ecam_region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
    bar_offset: u16,
) -> u32 {
    use super::mcfg::{read_config_u32, write_config_u32};

    // Save original BAR value
    let original = read_config_u32(ecam_region, bus, device, function, bar_offset);

    // Write all 1s to determine size
    write_config_u32(ecam_region, bus, device, function, bar_offset, 0xFFFFFFFF);
    let size_mask = read_config_u32(ecam_region, bus, device, function, bar_offset);

    // Restore original BAR value
    write_config_u32(ecam_region, bus, device, function, bar_offset, original);

    // Calculate size (mask off the lower 2 bits for I/O BARs)
    let size_mask = size_mask & 0xFFFFFFFC;

    if size_mask == 0 { 0 } else { (!size_mask) + 1 }
}
