//! NVMe controller register definitions
//!
//! This module defines the memory-mapped register layout for NVMe controllers
//! following the NVMe specification.

use x86_64::VirtAddr;

/// NVMe Controller Registers (mapped via BAR0)
#[repr(C)]
pub struct NvmeRegisters {
    // Controller Capabilities and Configuration (0x00-0x3F)
    pub cap: u64,           // 0x00: Controller Capabilities
    pub vs: u32,            // 0x08: Version
    pub intms: u32,         // 0x0C: Interrupt Mask Set
    pub intmc: u32,         // 0x10: Interrupt Mask Clear
    pub cc: u32,            // 0x14: Controller Configuration
    pub reserved1: u32,     // 0x18: Reserved
    pub csts: u32,          // 0x1C: Controller Status
    pub nssr: u32,          // 0x20: NVM Subsystem Reset
    pub aqa: u32,           // 0x24: Admin Queue Attributes
    pub asq: u64,           // 0x28: Admin Submission Queue Base Address
    pub acq: u64,           // 0x30: Admin Completion Queue Base Address
    pub cmbloc: u32,        // 0x38: Controller Memory Buffer Location
    pub cmbsz: u32,         // 0x3C: Controller Memory Buffer Size
    
    // Reserved space (0x40-0xFFF)
    pub _reserved: [u8; 0x1000 - 0x40],
    
    // Doorbell Registers start at 0x1000
    // Each queue pair has 2 doorbells (SQ and CQ)
    // Doorbell stride is determined by CAP.DSTRD
    pub doorbells: [u32; 256], // Support up to 128 queue pairs
}

impl NvmeRegisters {
    /// Create a new NvmeRegisters instance from a virtual address
    /// 
    /// # Safety
    /// The caller must ensure that the virtual address points to valid
    /// NVMe controller registers and remains valid for the lifetime of this struct.
    pub unsafe fn new(base_addr: VirtAddr) -> &'static mut Self {
        unsafe { &mut *(base_addr.as_mut_ptr::<Self>()) }
    }
    
    /// Get the maximum queue entries supported (CAP.MQES + 1)
    pub fn max_queue_entries(&self) -> u16 {
        ((self.cap & cap_bits::MQES_MASK) + 1) as u16
    }
    
    /// Get the doorbell stride in bytes (4 << CAP.DSTRD)
    pub fn doorbell_stride(&self) -> u32 {
        4 << ((self.cap >> cap_bits::DSTRD_SHIFT) & 0xF)
    }
    
    /// Get the minimum memory page size (4KB << CAP.MPSMIN)
    pub fn min_page_size(&self) -> u32 {
        4096 << ((self.cap >> cap_bits::MPSMIN_SHIFT) & 0xF)
    }
    
    /// Get the maximum memory page size (4KB << CAP.MPSMAX)
    pub fn max_page_size(&self) -> u32 {
        4096 << ((self.cap >> cap_bits::MPSMAX_SHIFT) & 0xF)
    }
    
    /// Check if the controller is ready
    pub fn is_ready(&self) -> bool {
        (self.csts & csts_bits::RDY) != 0
    }
    
    /// Check if the controller has a fatal status
    pub fn is_fatal(&self) -> bool {
        (self.csts & csts_bits::CFS) != 0
    }
    
    /// Enable the controller
    pub fn enable(&mut self) {
        self.cc |= cc_bits::EN;
    }
    
    /// Disable the controller
    pub fn disable(&mut self) {
        self.cc &= !cc_bits::EN;
    }
    
    /// Set admin queue attributes
    pub fn set_admin_queue_attributes(&mut self, sq_size: u16, cq_size: u16) {
        // Both sizes are 0-based (actual size - 1)
        self.aqa = ((cq_size - 1) as u32) << 16 | ((sq_size - 1) as u32);
    }
    
    /// Set admin submission queue base address
    pub fn set_admin_sq_base(&mut self, addr: u64) {
        self.asq = addr;
    }
    
    /// Set admin completion queue base address
    pub fn set_admin_cq_base(&mut self, addr: u64) {
        self.acq = addr;
    }
    
    /// Configure controller settings
    pub fn configure(&mut self) {
        let mut cc = 0;
        cc |= cc_bits::EN;                           // Enable controller
        cc |= 0 << cc_bits::CSS_SHIFT;               // NVM Command Set
        cc |= 0 << cc_bits::MPS_SHIFT;               // 4KB page size (2^(12+0))
        cc |= 0 << cc_bits::AMS_SHIFT;               // Round Robin arbitration
        cc |= 6 << cc_bits::IOSQES_SHIFT;            // 64-byte SQ entries (2^6)
        cc |= 4 << cc_bits::IOCQES_SHIFT;            // 16-byte CQ entries (2^4)
        
        self.cc = cc;
    }
    
    /// Ring doorbell for a specific queue
    pub fn ring_doorbell(&mut self, queue_id: u16, is_completion: bool, value: u16) {
        let doorbell_index = (queue_id * 2) + if is_completion { 1 } else { 0 };
        if (doorbell_index as usize) < self.doorbells.len() {
            unsafe {
                core::ptr::write_volatile(&mut self.doorbells[doorbell_index as usize], value as u32);
            }
        }
    }
}

/// Controller Capabilities Register (CAP) bit definitions
pub mod cap_bits {
    pub const MQES_MASK: u64 = 0xFFFF;           // Maximum Queue Entries Supported
    pub const CQR_SHIFT: u64 = 16;               // Contiguous Queues Required
    pub const AMS_MASK: u64 = 0x3 << 17;         // Arbitration Mechanism Supported
    pub const TO_SHIFT: u64 = 24;                // Timeout
    pub const DSTRD_SHIFT: u64 = 32;             // Doorbell Stride
    pub const NSSRS_SHIFT: u64 = 36;             // NVM Subsystem Reset Supported
    pub const CSS_MASK: u64 = 0xFF << 37;        // Command Sets Supported
    pub const BPS_SHIFT: u64 = 45;               // Boot Partition Support
    pub const MPSMIN_SHIFT: u64 = 48;            // Memory Page Size Minimum
    pub const MPSMAX_SHIFT: u64 = 52;            // Memory Page Size Maximum
}

/// Controller Configuration Register (CC) bit definitions
pub mod cc_bits {
    pub const EN: u32 = 1 << 0;                  // Enable
    pub const CSS_SHIFT: u32 = 4;                // I/O Command Set Selected
    pub const MPS_SHIFT: u32 = 7;                // Memory Page Size
    pub const AMS_SHIFT: u32 = 11;               // Arbitration Mechanism Selected
    pub const SHN_SHIFT: u32 = 14;               // Shutdown Notification
    pub const IOSQES_SHIFT: u32 = 16;            // I/O Submission Queue Entry Size
    pub const IOCQES_SHIFT: u32 = 20;            // I/O Completion Queue Entry Size
}

/// Controller Status Register (CSTS) bit definitions
pub mod csts_bits {
    pub const RDY: u32 = 1 << 0;                 // Ready
    pub const CFS: u32 = 1 << 1;                 // Controller Fatal Status
    pub const SHST_MASK: u32 = 0x3 << 2;         // Shutdown Status
    pub const NSSRO: u32 = 1 << 4;               // NVM Subsystem Reset Occurred
    pub const PP: u32 = 1 << 5;                  // Processing Paused
}

/// Admin Queue Attributes Register (AQA) bit definitions
pub mod aqa_bits {
    pub const ASQS_MASK: u32 = 0xFFF;            // Admin Submission Queue Size
    pub const ACQS_SHIFT: u32 = 16;              // Admin Completion Queue Size shift
    pub const ACQS_MASK: u32 = 0xFFF << ACQS_SHIFT; // Admin Completion Queue Size
}

/// NVMe command opcodes
pub mod opcodes {
    // Admin commands
    pub const ADMIN_DELETE_IO_SQ: u8 = 0x00;
    pub const ADMIN_CREATE_IO_SQ: u8 = 0x01;
    pub const ADMIN_GET_LOG_PAGE: u8 = 0x02;
    pub const ADMIN_DELETE_IO_CQ: u8 = 0x04;
    pub const ADMIN_CREATE_IO_CQ: u8 = 0x05;
    pub const ADMIN_IDENTIFY: u8 = 0x06;
    pub const ADMIN_ABORT: u8 = 0x08;
    pub const ADMIN_SET_FEATURES: u8 = 0x09;
    pub const ADMIN_GET_FEATURES: u8 = 0x0A;
    
    // NVM commands
    pub const NVM_FLUSH: u8 = 0x00;
    pub const NVM_WRITE: u8 = 0x01;
    pub const NVM_READ: u8 = 0x02;
    pub const NVM_WRITE_UNCORRECTABLE: u8 = 0x04;
    pub const NVM_COMPARE: u8 = 0x05;
    pub const NVM_WRITE_ZEROES: u8 = 0x08;
    pub const NVM_DATASET_MANAGEMENT: u8 = 0x09;
}

/// IDENTIFY command CNS (Controller or Namespace Structure) values
pub mod identify_cns {
    pub const NAMESPACE: u32 = 0x00;             // Identify Namespace
    pub const CONTROLLER: u32 = 0x01;            // Identify Controller
    pub const NAMESPACE_LIST: u32 = 0x02;        // Active Namespace ID list
    pub const NAMESPACE_DESCRIPTOR: u32 = 0x03;  // Namespace Identification Descriptor
}
