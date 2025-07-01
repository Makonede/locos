//! PCIe configuration space definitions and utilities.
//!
//! This module provides:
//! - Standard PCIe configuration space layout definitions
//! - Command and status register bit definitions
//! - Capability ID constants
//! - Helper functions for configuration space manipulation

/// PCIe Command Register bits
pub mod command_bits {
    pub const IO_SPACE: u16 = 1 << 0;
    pub const MEMORY_SPACE: u16 = 1 << 1;
    pub const BUS_MASTER: u16 = 1 << 2;
    pub const SPECIAL_CYCLES: u16 = 1 << 3;
    pub const MEMORY_WRITE_INVALIDATE: u16 = 1 << 4;
    pub const VGA_PALETTE_SNOOP: u16 = 1 << 5;
    pub const PARITY_ERROR_RESPONSE: u16 = 1 << 6;
    pub const SERR_ENABLE: u16 = 1 << 8;
    pub const FAST_BACK_TO_BACK: u16 = 1 << 9;
    pub const INTERRUPT_DISABLE: u16 = 1 << 10;
}

/// PCIe Status Register bits
pub mod status_bits {
    pub const INTERRUPT_STATUS: u16 = 1 << 3;
    pub const CAPABILITIES_LIST: u16 = 1 << 4;
    pub const MHZ66_CAPABLE: u16 = 1 << 5;
    pub const FAST_BACK_TO_BACK: u16 = 1 << 7;
    pub const MASTER_DATA_PARITY_ERROR: u16 = 1 << 8;
    pub const DEVSEL_TIMING_MASK: u16 = 0x3 << 9;
    pub const SIGNALED_TARGET_ABORT: u16 = 1 << 11;
    pub const RECEIVED_TARGET_ABORT: u16 = 1 << 12;
    pub const RECEIVED_MASTER_ABORT: u16 = 1 << 13;
    pub const SIGNALED_SYSTEM_ERROR: u16 = 1 << 14;
    pub const DETECTED_PARITY_ERROR: u16 = 1 << 15;
}

/// PCIe Capability IDs
pub mod capability_ids {
    pub const POWER_MANAGEMENT: u8 = 0x01;
    pub const AGP: u8 = 0x02;
    pub const VPD: u8 = 0x03;
    pub const SLOT_ID: u8 = 0x04;
    pub const MSI: u8 = 0x05;
    pub const COMPACT_PCI_HOT_SWAP: u8 = 0x06;
    pub const PCI_X: u8 = 0x07;
    pub const HYPER_TRANSPORT: u8 = 0x08;
    pub const VENDOR_SPECIFIC: u8 = 0x09;
    pub const DEBUG_PORT: u8 = 0x0A;
    pub const COMPACT_PCI_CRC: u8 = 0x0B;
    pub const PCI_HOT_PLUG: u8 = 0x0C;
    pub const PCI_BRIDGE_SUBSYSTEM_VID: u8 = 0x0D;
    pub const AGP_8X: u8 = 0x0E;
    pub const SECURE_DEVICE: u8 = 0x0F;
    pub const PCI_EXPRESS: u8 = 0x10;
    pub const MSI_X: u8 = 0x11;
    pub const SATA_DATA_INDEX_CONFIG: u8 = 0x12;
    pub const ADVANCED_FEATURES: u8 = 0x13;
    pub const ENHANCED_ALLOCATION: u8 = 0x14;
}

/// PCIe Extended Capability IDs (for PCIe extended configuration space)
pub mod extended_capability_ids {
    pub const NULL: u16 = 0x0000;
    pub const ADVANCED_ERROR_REPORTING: u16 = 0x0001;
    pub const VIRTUAL_CHANNEL: u16 = 0x0002;
    pub const DEVICE_SERIAL_NUMBER: u16 = 0x0003;
    pub const POWER_BUDGETING: u16 = 0x0004;
    pub const ROOT_COMPLEX_LINK_DECLARATION: u16 = 0x0005;
    pub const ROOT_COMPLEX_INTERNAL_LINK_CONTROL: u16 = 0x0006;
    pub const ROOT_COMPLEX_EVENT_COLLECTOR: u16 = 0x0007;
    pub const MULTI_FUNCTION_VIRTUAL_CHANNEL: u16 = 0x0008;
    pub const VIRTUAL_CHANNEL_2: u16 = 0x0009;
    pub const ROOT_COMPLEX_REGISTER_BLOCK: u16 = 0x000A;
    pub const VENDOR_SPECIFIC_EXTENDED: u16 = 0x000B;
    pub const CONFIGURATION_ACCESS_CORRELATION: u16 = 0x000C;
    pub const ACCESS_CONTROL_SERVICES: u16 = 0x000D;
    pub const ALTERNATIVE_ROUTING_ID_INTERPRETATION: u16 = 0x000E;
    pub const ADDRESS_TRANSLATION_SERVICES: u16 = 0x000F;
    pub const SINGLE_ROOT_IO_VIRTUALIZATION: u16 = 0x0010;
    pub const MULTI_ROOT_IO_VIRTUALIZATION: u16 = 0x0011;
    pub const MULTICAST: u16 = 0x0012;
    pub const PAGE_REQUEST_INTERFACE: u16 = 0x0013;
    pub const RESERVED_FOR_AMD: u16 = 0x0014;
    pub const RESIZABLE_BAR: u16 = 0x0015;
    pub const DYNAMIC_POWER_ALLOCATION: u16 = 0x0016;
    pub const TPH_REQUESTER: u16 = 0x0017;
    pub const LATENCY_TOLERANCE_REPORTING: u16 = 0x0018;
    pub const SECONDARY_PCI_EXPRESS: u16 = 0x0019;
    pub const PROTOCOL_MULTIPLEXING: u16 = 0x001A;
    pub const PROCESS_ADDRESS_SPACE_ID: u16 = 0x001B;
    pub const LN_REQUESTER: u16 = 0x001C;
    pub const DOWNSTREAM_PORT_CONTAINMENT: u16 = 0x001D;
    pub const L1_PM_SUBSTATES: u16 = 0x001E;
    pub const PRECISION_TIME_MEASUREMENT: u16 = 0x001F;
    pub const PCI_EXPRESS_OVER_M_PHY: u16 = 0x0020;
    pub const FRS_QUEUEING: u16 = 0x0021;
    pub const READINESS_TIME_REPORTING: u16 = 0x0022;
    pub const DESIGNATED_VENDOR_SPECIFIC: u16 = 0x0023;
    pub const VF_RESIZABLE_BAR: u16 = 0x0024;
    pub const DATA_LINK_FEATURE: u16 = 0x0025;
    pub const PHYSICAL_LAYER_16_0_GT_S: u16 = 0x0026;
    pub const LANE_MARGINING_AT_RECEIVER: u16 = 0x0027;
    pub const HIERARCHY_ID: u16 = 0x0028;
    pub const NATIVE_PCI_EXPRESS_ENCLOSURE: u16 = 0x0029;
    pub const PHYSICAL_LAYER_32_0_GT_S: u16 = 0x002A;
    pub const ALTERNATE_PROTOCOL: u16 = 0x002B;
    pub const SYSTEM_FIRMWARE_INTERMEDIARY: u16 = 0x002C;
}

/// PCIe device classes
pub mod device_classes {
    pub const UNCLASSIFIED: u8 = 0x00;
    pub const MASS_STORAGE: u8 = 0x01;
    pub const NETWORK: u8 = 0x02;
    pub const DISPLAY: u8 = 0x03;
    pub const MULTIMEDIA: u8 = 0x04;
    pub const MEMORY: u8 = 0x05;
    pub const BRIDGE: u8 = 0x06;
    pub const COMMUNICATION: u8 = 0x07;
    pub const SYSTEM_PERIPHERAL: u8 = 0x08;
    pub const INPUT_DEVICE: u8 = 0x09;
    pub const DOCKING_STATION: u8 = 0x0A;
    pub const PROCESSOR: u8 = 0x0B;
    pub const SERIAL_BUS: u8 = 0x0C;
    pub const WIRELESS: u8 = 0x0D;
    pub const INTELLIGENT_IO: u8 = 0x0E;
    pub const SATELLITE_COMMUNICATION: u8 = 0x0F;
    pub const ENCRYPTION: u8 = 0x10;
    pub const DATA_ACQUISITION: u8 = 0x11;
    pub const PROCESSING_ACCELERATOR: u8 = 0x12;
    pub const NON_ESSENTIAL_INSTRUMENTATION: u8 = 0x13;
    pub const COPROCESSOR: u8 = 0x40;
    pub const UNASSIGNED: u8 = 0xFF;
}

/// Common vendor IDs
pub mod vendor_ids {
    pub const INTEL: u16 = 0x8086;
    pub const AMD: u16 = 0x1022;
    pub const NVIDIA: u16 = 0x10DE;
    pub const BROADCOM: u16 = 0x14E4;
    pub const QUALCOMM: u16 = 0x17CB;
    pub const MARVELL: u16 = 0x11AB;
    pub const REALTEK: u16 = 0x10EC;
    pub const VIA: u16 = 0x1106;
    pub const SILICON_IMAGE: u16 = 0x1095;
    pub const PROMISE: u16 = 0x105A;
    pub const ADAPTEC: u16 = 0x9004;
    pub const LSI_LOGIC: u16 = 0x1000;
    pub const DELL: u16 = 0x1028;
    pub const HP: u16 = 0x103C;
    pub const IBM: u16 = 0x1014;
    pub const MICROSOFT: u16 = 0x1414;
    pub const VMWARE: u16 = 0x15AD;
    pub const QEMU: u16 = 0x1234;
    pub const REDHAT: u16 = 0x1AF4;
}

/// BAR type definitions
pub mod bar_types {
    /// Memory BAR type bits (bits 1-2)
    pub const MEMORY_TYPE_32BIT: u32 = 0x0;
    pub const MEMORY_TYPE_1MB: u32 = 0x2;
    pub const MEMORY_TYPE_64BIT: u32 = 0x4;
    
    /// Memory BAR prefetchable bit (bit 3)
    pub const MEMORY_PREFETCHABLE: u32 = 0x8;
    
    /// BAR type bit (bit 0)
    pub const BAR_TYPE_MEMORY: u32 = 0x0;
    pub const BAR_TYPE_IO: u32 = 0x1;
    
    /// Memory BAR address mask
    pub const MEMORY_BAR_MASK: u32 = 0xFFFFFFF0;
    
    /// I/O BAR address mask
    pub const IO_BAR_MASK: u32 = 0xFFFFFFFC;
}

/// MSI capability structure offsets
pub mod msi_offsets {
    pub const CAPABILITY_ID: u16 = 0x00;
    pub const NEXT_POINTER: u16 = 0x01;
    pub const MESSAGE_CONTROL: u16 = 0x02;
    pub const MESSAGE_ADDRESS_LOW: u16 = 0x04;
    pub const MESSAGE_ADDRESS_HIGH: u16 = 0x08; // Only present if 64-bit capable
    pub const MESSAGE_DATA_32: u16 = 0x08;      // For 32-bit MSI
    pub const MESSAGE_DATA_64: u16 = 0x0C;      // For 64-bit MSI
    pub const MASK_BITS_32: u16 = 0x0C;         // For 32-bit MSI with per-vector masking
    pub const MASK_BITS_64: u16 = 0x10;         // For 64-bit MSI with per-vector masking
    pub const PENDING_BITS_32: u16 = 0x10;      // For 32-bit MSI with per-vector masking
    pub const PENDING_BITS_64: u16 = 0x14;      // For 64-bit MSI with per-vector masking
}

/// MSI-X capability structure offsets
pub mod msix_offsets {
    pub const CAPABILITY_ID: u16 = 0x00;
    pub const NEXT_POINTER: u16 = 0x01;
    pub const MESSAGE_CONTROL: u16 = 0x02;
    pub const TABLE_OFFSET_BIR: u16 = 0x04;
    pub const PBA_OFFSET_BIR: u16 = 0x08;
}

/// MSI Message Control register bits
pub mod msi_control_bits {
    pub const MSI_ENABLE: u16 = 1 << 0;
    pub const MULTIPLE_MESSAGE_CAPABLE_MASK: u16 = 0x7 << 1;
    pub const MULTIPLE_MESSAGE_ENABLE_MASK: u16 = 0x7 << 4;
    pub const ADDRESS_64_CAPABLE: u16 = 1 << 7;
    pub const PER_VECTOR_MASKING_CAPABLE: u16 = 1 << 8;
}

/// MSI-X Message Control register bits
pub mod msix_control_bits {
    pub const TABLE_SIZE_MASK: u16 = 0x7FF;
    pub const FUNCTION_MASK: u16 = 1 << 14;
    pub const MSI_X_ENABLE: u16 = 1 << 15;
}

/// MSI-X Table Entry structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MsiXTableEntry {
    pub message_address_low: u32,
    pub message_address_high: u32,
    pub message_data: u32,
    pub vector_control: u32,
}

impl MsiXTableEntry {
    pub const VECTOR_MASKED: u32 = 1 << 0;
    
    pub fn new() -> Self {
        Self {
            message_address_low: 0,
            message_address_high: 0,
            message_data: 0,
            vector_control: Self::VECTOR_MASKED,
        }
    }
    
    pub fn set_address(&mut self, address: u64) {
        self.message_address_low = address as u32;
        self.message_address_high = (address >> 32) as u32;
    }
    
    pub fn set_data(&mut self, data: u32) {
        self.message_data = data;
    }
    
    pub fn mask(&mut self) {
        self.vector_control |= Self::VECTOR_MASKED;
    }
    
    pub fn unmask(&mut self) {
        self.vector_control &= !Self::VECTOR_MASKED;
    }
    
    pub fn is_masked(&self) -> bool {
        (self.vector_control & Self::VECTOR_MASKED) != 0
    }
}

impl Default for MsiXTableEntry {
    fn default() -> Self {
        Self::new()
    }
}
