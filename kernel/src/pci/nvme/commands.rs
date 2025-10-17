//! NVMe command structures and helpers
//!
//! This module provides command and completion structures for NVMe operations,
//! following the same pattern as the xHCI TRB helpers.

use super::registers::opcodes;

/// NVMe Submission Queue Entry (64 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NvmeCommand {
    pub cdw0: u32,          // Command Dword 0 (Opcode, Flags, CID)
    pub nsid: u32,          // Namespace Identifier
    pub cdw2: u32,          // Command Dword 2
    pub cdw3: u32,          // Command Dword 3
    pub mptr: u64,          // Metadata Pointer
    pub prp1: u64,          // PRP Entry 1 (Physical Region Page)
    pub prp2: u64,          // PRP Entry 2
    pub cdw10: u32,         // Command Dword 10
    pub cdw11: u32,         // Command Dword 11
    pub cdw12: u32,         // Command Dword 12
    pub cdw13: u32,         // Command Dword 13
    pub cdw14: u32,         // Command Dword 14
    pub cdw15: u32,         // Command Dword 15
}

/// NVMe Completion Queue Entry (16 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NvmeCompletion {
    pub dw0: u32,           // Command Specific
    pub dw1: u32,           // Reserved
    pub sq_head: u16,       // Submission Queue Head Pointer
    pub sq_id: u16,         // Submission Queue Identifier
    pub cid: u16,           // Command Identifier
    pub status: u16,        // Status Field (Phase bit + Status Code)
}

impl NvmeCommand {
    /// Create a new command with all fields zeroed
    pub const fn new() -> Self {
        Self {
            cdw0: 0, nsid: 0, cdw2: 0, cdw3: 0, mptr: 0,
            prp1: 0, prp2: 0, cdw10: 0, cdw11: 0, cdw12: 0,
            cdw13: 0, cdw14: 0, cdw15: 0,
        }
    }
    
    /// Get the opcode from CDW0 (bits 0-7)
    pub fn opcode(&self) -> u8 {
        (self.cdw0 & 0xFF) as u8
    }
    
    /// Set the opcode in CDW0 (bits 0-7)
    pub fn set_opcode(&mut self, opcode: u8) {
        self.cdw0 = (self.cdw0 & !0xFF) | (opcode as u32);
    }
    
    /// Get the command identifier from CDW0 (bits 16-31)
    pub fn command_id(&self) -> u16 {
        ((self.cdw0 >> 16) & 0xFFFF) as u16
    }
    
    /// Set the command identifier in CDW0 (bits 16-31)
    pub fn set_command_id(&mut self, cid: u16) {
        self.cdw0 = (self.cdw0 & 0x0000FFFF) | ((cid as u32) << 16);
    }
    
    /// Create an IDENTIFY Controller command
    pub fn identify_controller(buffer_addr: u64) -> Self {
        let mut cmd = Self::new();
        cmd.set_opcode(opcodes::ADMIN_IDENTIFY);
        cmd.nsid = 0;                    // Controller identify
        cmd.prp1 = buffer_addr;
        cmd.cdw10 = 1;                   // CNS = 1 (Controller)
        cmd
    }
    
    /// Create an IDENTIFY Namespace command
    pub fn identify_namespace(nsid: u32, buffer_addr: u64) -> Self {
        let mut cmd = Self::new();
        cmd.set_opcode(opcodes::ADMIN_IDENTIFY);
        cmd.nsid = nsid;
        cmd.prp1 = buffer_addr;
        cmd.cdw10 = 0;                   // CNS = 0 (Namespace)
        cmd
    }
    
    /// Create a CREATE I/O Completion Queue command
    pub fn create_io_cq(queue_id: u16, queue_size: u16, buffer_addr: u64) -> Self {
        let mut cmd = Self::new();
        cmd.set_opcode(opcodes::ADMIN_CREATE_IO_CQ);
        cmd.prp1 = buffer_addr;
        cmd.cdw10 = ((queue_size - 1) as u32) << 16 | (queue_id as u32); // QSIZE | QID
        cmd.cdw11 = 1;                   // PC = 1 (Physically Contiguous), IEN = 0 (no interrupts)
        cmd
    }

    /// Create a CREATE I/O Completion Queue command with MSI-X interrupt
    pub fn create_io_cq_with_interrupt(queue_id: u16, queue_size: u16, buffer_addr: u64, interrupt_vector: u16) -> Self {
        let mut cmd = Self::new();
        cmd.set_opcode(opcodes::ADMIN_CREATE_IO_CQ);
        cmd.prp1 = buffer_addr;
        cmd.cdw10 = ((queue_size - 1) as u32) << 16 | (queue_id as u32); // QSIZE | QID
        // PC = 1 (Physically Contiguous), IEN = 1 (interrupts enabled)
        cmd.cdw11 = ((interrupt_vector as u32) << 16) | (1 << 1) | 1; // IV | IEN | PC
        cmd
    }
    
    /// Create a CREATE I/O Submission Queue command
    pub fn create_io_sq(queue_id: u16, cq_id: u16, queue_size: u16, buffer_addr: u64) -> Self {
        let mut cmd = Self::new();
        cmd.set_opcode(opcodes::ADMIN_CREATE_IO_SQ);
        cmd.prp1 = buffer_addr;
        cmd.cdw10 = ((queue_size - 1) as u32) << 16 | (queue_id as u32); // QSIZE | QID
        cmd.cdw11 = (cq_id as u32) << 16 | 1;        // CQID | PC = 1
        cmd
    }
    
    /// Create a READ command
    pub fn read(nsid: u32, lba: u64, blocks: u16, buffer_addr: u64) -> Self {
        let mut cmd = Self::new();
        cmd.set_opcode(opcodes::NVM_READ);
        cmd.nsid = nsid;
        cmd.prp1 = buffer_addr;
        cmd.cdw10 = lba as u32;                    // SLBA (lower 32 bits)
        cmd.cdw11 = (lba >> 32) as u32;           // SLBA (upper 32 bits)
        cmd.cdw12 = (blocks - 1) as u32;          // NLB (0-based)
        cmd
    }
    
    /// Create a WRITE command
    pub fn write(nsid: u32, lba: u64, blocks: u16, buffer_addr: u64) -> Self {
        let mut cmd = Self::new();
        cmd.set_opcode(opcodes::NVM_WRITE);
        cmd.nsid = nsid;
        cmd.prp1 = buffer_addr;
        cmd.cdw10 = lba as u32;                    // SLBA (lower 32 bits)
        cmd.cdw11 = (lba >> 32) as u32;           // SLBA (upper 32 bits)
        cmd.cdw12 = (blocks - 1) as u32;          // NLB (0-based)
        cmd
    }
    
    /// Set up PRP2 for transfers larger than one page
    pub fn set_prp2(&mut self, addr: u64) {
        self.prp2 = addr;
    }
}

impl NvmeCompletion {
    /// Get the status code (bits 1-15 of status field)
    pub fn status_code(&self) -> u16 {
        (self.status >> 1) & 0x7FFF
    }
    
    /// Get the phase bit (bit 0 of status field)
    pub fn phase_bit(&self) -> bool {
        (self.status & 1) != 0
    }
    
    /// Check if the command completed successfully
    pub fn is_success(&self) -> bool {
        self.status_code() == 0
    }
    
    /// Check if this completion entry is valid (has expected phase bit)
    pub fn is_valid(&self, expected_phase: bool) -> bool {
        self.phase_bit() == expected_phase
    }
}

/// Controller Identify Data Structure (4096 bytes)
/// This is a simplified version with only the most important fields
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IdentifyController {
    pub vid: u16,           // PCI Vendor ID
    pub ssvid: u16,         // PCI Subsystem Vendor ID
    pub sn: [u8; 20],       // Serial Number
    pub mn: [u8; 40],       // Model Number
    pub fr: [u8; 8],        // Firmware Revision
    pub rab: u8,            // Recommended Arbitration Burst
    pub ieee: [u8; 3],      // IEEE OUI Identifier
    pub cmic: u8,           // Controller Multi-Path I/O and Namespace Sharing
    pub mdts: u8,           // Maximum Data Transfer Size
    pub cntlid: u16,        // Controller ID
    pub ver: u32,           // Version
    pub rtd3r: u32,         // RTD3 Resume Latency
    pub rtd3e: u32,         // RTD3 Entry Latency
    pub oaes: u32,          // Optional Asynchronous Events Supported
    pub ctratt: u32,        // Controller Attributes
    pub rrls: u16,          // Read Recovery Levels Supported
    pub _reserved1: [u8; 9],
    pub cntrltype: u8,      // Controller Type
    pub fguid: [u8; 16],    // FRU Globally Unique Identifier
    pub crdt1: u16,         // Command Retry Delay Time 1
    pub crdt2: u16,         // Command Retry Delay Time 2
    pub crdt3: u16,         // Command Retry Delay Time 3
    pub _reserved2: [u8; 122],
    
    // Admin Command Set Attributes & Optional Controller Capabilities (256-511)
    pub oacs: u16,          // Optional Admin Command Support
    pub acl: u8,            // Abort Command Limit
    pub aerl: u8,           // Asynchronous Event Request Limit
    pub frmw: u8,           // Firmware Updates
    pub lpa: u8,            // Log Page Attributes
    pub elpe: u8,           // Error Log Page Entries
    pub npss: u8,           // Number of Power States Support
    pub avscc: u8,          // Admin Vendor Specific Command Configuration
    pub apsta: u8,          // Autonomous Power State Transition Attributes
    pub wctemp: u16,        // Warning Composite Temperature Threshold
    pub cctemp: u16,        // Critical Composite Temperature Threshold
    pub mtfa: u16,          // Maximum Time for Firmware Activation
    pub hmpre: u32,         // Host Memory Buffer Preferred Size
    pub hmmin: u32,         // Host Memory Buffer Minimum Size
    pub tnvmcap: [u8; 16],  // Total NVM Capacity
    pub unvmcap: [u8; 16],  // Unallocated NVM Capacity
    pub rpmbs: u32,         // Replay Protected Memory Block Support
    pub edstt: u16,         // Extended Device Self-test Time
    pub dsto: u8,           // Device Self-test Options
    pub fwug: u8,           // Firmware Update Granularity
    pub kas: u16,           // Keep Alive Support
    pub hctma: u16,         // Host Controlled Thermal Management Attributes
    pub mntmt: u16,         // Minimum Thermal Management Temperature
    pub mxtmt: u16,         // Maximum Thermal Management Temperature
    pub sanicap: u32,       // Sanitize Capabilities
    pub hmminds: u32,       // Host Memory Buffer Minimum Descriptor Entry Size
    pub hmmaxd: u16,        // Host Memory Maximum Descriptors Entries
    pub nsetidmax: u16,     // NVM Set Identifier Maximum
    pub endgidmax: u16,     // Endurance Group Identifier Maximum
    pub anatt: u8,          // ANA Transition Time
    pub anacap: u8,         // Asymmetric Namespace Access Capabilities
    pub anagrpmax: u32,     // ANA Group Identifier Maximum
    pub nanagrpid: u32,     // Number of ANA Group Identifiers
    pub pels: u32,          // Persistent Event Log Size
    pub _reserved3: [u8; 156],
    
    // NVM Command Set Attributes (512-703)
    pub sqes: u8,           // Submission Queue Entry Size
    pub cqes: u8,           // Completion Queue Entry Size
    pub maxcmd: u16,        // Maximum Outstanding Commands
    pub nn: u32,            // Number of Namespaces
    pub oncs: u16,          // Optional NVM Command Support
    pub fuses: u16,         // Fused Operation Support
    pub fna: u8,            // Format NVM Attributes
    pub vwc: u8,            // Volatile Write Cache
    pub awun: u16,          // Atomic Write Unit Normal
    pub awupf: u16,         // Atomic Write Unit Power Fail
    pub nvscc: u8,          // NVM Vendor Specific Command Configuration
    pub nwpc: u8,           // Namespace Write Protection Capabilities
    pub acwu: u16,          // Atomic Compare & Write Unit
    pub _reserved4: [u8; 2],
    pub sgls: u32,          // SGL Support
    pub mnan: u32,          // Maximum Number of Allowed Namespaces
    pub _reserved5: [u8; 224],
    
    // I/O Command Set Independent Attributes (704-2047)
    pub subnqn: [u8; 256],  // NVM Subsystem NVMe Qualified Name
    pub _reserved6: [u8; 768],
    
    // NVMe over Fabrics Attributes (2048-2303)
    pub ioccsz: u32,        // I/O Queue Command Capsule Supported Size
    pub iorcsz: u32,        // I/O Queue Response Capsule Supported Size
    pub icdoff: u16,        // In Capsule Data Offset
    pub fcatt: u8,          // Fabrics Controller Attributes
    pub msdbd: u8,          // Maximum SGL Data Block Descriptors
    pub ofcs: u16,          // Optional Fabric Commands Support
    pub _reserved7: [u8; 242],
    
    // Power State Descriptors (2304-3071)
    pub psd: [u8; 768],     // Power State Descriptors
    
    // Vendor Specific (3072-4095)
    pub vs: [u8; 1024],     // Vendor Specific
}

/// Namespace Identify Data Structure (4096 bytes)
/// Simplified version with essential fields
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IdentifyNamespace {
    pub nsze: u64,          // Namespace Size
    pub ncap: u64,          // Namespace Capacity
    pub nuse: u64,          // Namespace Utilization
    pub nsfeat: u8,         // Namespace Features
    pub nlbaf: u8,          // Number of LBA Formats
    pub flbas: u8,          // Formatted LBA Size
    pub mc: u8,             // Metadata Capabilities
    pub dpc: u8,            // End-to-end Data Protection Capabilities
    pub dps: u8,            // End-to-end Data Protection Type Settings
    pub nmic: u8,           // Namespace Multi-path I/O and Namespace Sharing
    pub rescap: u8,         // Reservation Capabilities
    pub fpi: u8,            // Format Progress Indicator
    pub dlfeat: u8,         // Deallocate Logical Block Features
    pub nawun: u16,         // Namespace Atomic Write Unit Normal
    pub nawupf: u16,        // Namespace Atomic Write Unit Power Fail
    pub nacwu: u16,         // Namespace Atomic Compare & Write Unit
    pub nabsn: u16,         // Namespace Atomic Boundary Size Normal
    pub nabo: u16,          // Namespace Atomic Boundary Offset
    pub nabspf: u16,        // Namespace Atomic Boundary Size Power Fail
    pub noiob: u16,         // Namespace Optimal I/O Boundary
    pub nvmcap: [u8; 16],   // NVM Capacity
    pub npwg: u16,          // Namespace Preferred Write Granularity
    pub npwa: u16,          // Namespace Preferred Write Alignment
    pub npdg: u16,          // Namespace Preferred Deallocate Granularity
    pub npda: u16,          // Namespace Preferred Deallocate Alignment
    pub nows: u16,          // Namespace Optimal Write Size
    pub _reserved1: [u8; 18],
    pub anagrpid: u32,      // ANA Group Identifier
    pub _reserved2: [u8; 3],
    pub nsattr: u8,         // Namespace Attributes
    pub nvmsetid: u16,      // NVM Set Identifier
    pub endgid: u16,        // Endurance Group Identifier
    pub nguid: [u8; 16],    // Namespace Globally Unique Identifier
    pub eui64: [u8; 8],     // IEEE Extended Unique Identifier
    
    // LBA Format Support (128-191)
    pub lbaf: [LbaFormat; 16], // LBA Format Support
    
    // Reserved (192-383)
    pub _reserved3: [u8; 192],
    
    // Vendor Specific (384-4095)
    pub vs: [u8; 3712],     // Vendor Specific
}

/// LBA Format Data Structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LbaFormat {
    pub ms: u16,            // Metadata Size
    pub lbads: u8,          // LBA Data Size (2^n bytes)
    pub rp: u8,             // Relative Performance
}

impl IdentifyNamespace {
    /// Get the LBA size in bytes for the current format
    pub fn lba_size(&self) -> u32 {
        let format_index = (self.flbas & 0x0F) as usize;
        if format_index < self.lbaf.len() {
            1 << self.lbaf[format_index].lbads
        } else {
            512 // Default to 512 bytes
        }
    }
    
    /// Get the namespace size in bytes
    pub fn size_bytes(&self) -> u64 {
        self.nsze * self.lba_size() as u64
    }
}
