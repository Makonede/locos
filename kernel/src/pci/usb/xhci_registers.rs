//! xHCI (eXtensible Host Controller Interface) register definitions and access functions.
//!
//! This module provides safe abstractions for accessing xHCI MMIO registers
//! based on the xHCI specification and OSDev wiki documentation.

use core::ptr::{read_volatile, write_volatile};
use x86_64::VirtAddr;

/// xHCI Host Controller Capability Registers (read-only)
/// These registers define the capabilities and limits of the host controller
#[repr(C)]
pub struct CapabilityRegisters {
    /// Capability Register Length (CAPLENGTH) - 8 bits
    /// Length of the capability register space
    pub cap_length: u8,

    /// Reserved - 8 bits
    _reserved1: u8,

    /// Host Controller Interface Version Number (HCIVERSION) - 16 bits
    /// BCD encoding of the xHCI specification version
    pub hci_version: u16,

    /// Host Controller Structural Parameters 1 (HCSPARAMS1) - 32 bits
    pub hcs_params1: HcsParams1,

    /// Host Controller Structural Parameters 2 (HCSPARAMS2) - 32 bits
    pub hcs_params2: HcsParams2,

    /// Host Controller Structural Parameters 3 (HCSPARAMS3) - 32 bits
    pub hcs_params3: HcsParams3,

    /// Host Controller Capability Parameters 1 (HCCPARAMS1) - 32 bits
    pub hcc_params1: HccParams1,

    /// Doorbell Offset (DBOFF) - 32 bits
    /// Offset to doorbell array from the base address
    pub doorbell_offset: u32,

    /// Runtime Register Space Offset (RTSOFF) - 32 bits
    /// Offset to runtime registers from the base address
    pub runtime_offset: u32,

    /// Host Controller Capability Parameters 2 (HCCPARAMS2) - 32 bits
    pub hcc_params2: HccParams2,
}

/// Host Controller Structural Parameters 1
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct HcsParams1(pub u32);

impl HcsParams1 {
    /// Maximum number of device slots (1-255)
    pub fn max_device_slots(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    /// Maximum number of interrupters (1-1023)
    pub fn max_interrupters(&self) -> u16 {
        ((self.0 >> 8) & 0x7FF) as u16
    }

    /// Maximum number of ports (1-255)
    pub fn max_ports(&self) -> u8 {
        ((self.0 >> 24) & 0xFF) as u8
    }
}

/// Host Controller Structural Parameters 2
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct HcsParams2(pub u32);

impl HcsParams2 {
    /// Isochronous Scheduling Threshold (IST)
    pub fn ist(&self) -> u8 {
        (self.0 & 0xF) as u8
    }

    /// Event Ring Segment Table Max (ERSTMAX)
    pub fn erst_max(&self) -> u16 {
        ((self.0 >> 4) & 0xF) as u16
    }

    /// Max Scratchpad Buffers
    pub fn max_scratchpad_buffers(&self) -> u16 {
        let hi = ((self.0 >> 21) & 0x1F) as u16;
        let lo = ((self.0 >> 27) & 0x1F) as u16;
        (hi << 5) | lo
    }
}

/// Host Controller Structural Parameters 3
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct HcsParams3(pub u32);

impl HcsParams3 {
    /// U1 Device Exit Latency
    pub fn u1_device_exit_latency(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    /// U2 Device Exit Latency
    pub fn u2_device_exit_latency(&self) -> u16 {
        ((self.0 >> 16) & 0xFFFF) as u16
    }
}

/// Host Controller Capability Parameters 1
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct HccParams1(pub u32);

impl HccParams1 {
    /// 64-bit Addressing Capability
    pub fn ac64(&self) -> bool {
        (self.0 & 0x1) != 0
    }

    /// Bandwidth Negotiation Capability
    pub fn bnc(&self) -> bool {
        (self.0 & 0x2) != 0
    }

    /// Context Size (0 = 32 bytes, 1 = 64 bytes)
    pub fn csz(&self) -> bool {
        (self.0 & 0x4) != 0
    }

    /// Port Power Control
    pub fn ppc(&self) -> bool {
        (self.0 & 0x8) != 0
    }

    /// Port Indicators
    pub fn pind(&self) -> bool {
        (self.0 & 0x10) != 0
    }

    /// Light HC Reset Capability
    pub fn lhrc(&self) -> bool {
        (self.0 & 0x20) != 0
    }

    /// Latency Tolerance Messaging Capability
    pub fn ltc(&self) -> bool {
        (self.0 & 0x40) != 0
    }

    /// No Secondary SID Support
    pub fn nss(&self) -> bool {
        (self.0 & 0x80) != 0
    }

    /// Parse All Event Data
    pub fn pae(&self) -> bool {
        (self.0 & 0x100) != 0
    }

    /// Stopped - Short Packet Capability
    pub fn spc(&self) -> bool {
        (self.0 & 0x200) != 0
    }

    /// Stopped EDTLA Capability
    pub fn sec(&self) -> bool {
        (self.0 & 0x400) != 0
    }

    /// Contiguous Frame ID Capability
    pub fn cfc(&self) -> bool {
        (self.0 & 0x800) != 0
    }

    /// Maximum Primary Stream Array Size
    pub fn max_psasize(&self) -> u8 {
        ((self.0 >> 12) & 0xF) as u8
    }

    /// xHCI Extended Capabilities Pointer
    pub fn xecp(&self) -> u16 {
        ((self.0 >> 16) & 0xFFFF) as u16
    }
}

/// Host Controller Capability Parameters 2
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct HccParams2(pub u32);

impl HccParams2 {
    /// U3 Entry Capability
    pub fn u3c(&self) -> bool {
        (self.0 & 0x1) != 0
    }

    /// Configure Endpoint Command Max Exit Latency Too Large Capability
    pub fn cmc(&self) -> bool {
        (self.0 & 0x2) != 0
    }

    /// Force Save Context Capability
    pub fn fsc(&self) -> bool {
        (self.0 & 0x4) != 0
    }

    /// Compliance Transition Capability
    pub fn ctc(&self) -> bool {
        (self.0 & 0x8) != 0
    }

    /// Large ESIT Payload Capability
    pub fn lec(&self) -> bool {
        (self.0 & 0x10) != 0
    }

    /// Configuration Information Capability
    pub fn cic(&self) -> bool {
        (self.0 & 0x20) != 0
    }

    /// Extended TBC Capability
    pub fn etc(&self) -> bool {
        (self.0 & 0x40) != 0
    }

    /// Extended TBC TRB Status Capability
    pub fn etc_tsc(&self) -> bool {
        (self.0 & 0x80) != 0
    }

    /// Get/Set Extended Property Capability
    pub fn gsc(&self) -> bool {
        (self.0 & 0x100) != 0
    }

    /// Virtualization Based Trusted I/O Capability
    pub fn vtc(&self) -> bool {
        (self.0 & 0x200) != 0
    }
}

/// xHCI Host Controller Operational Registers
/// These registers control the operation of the host controller
#[repr(C)]
pub struct OperationalRegisters {
    /// USB Command Register (USBCMD) - 32 bits
    pub usb_cmd: UsbCmd,

    /// USB Status Register (USBSTS) - 32 bits
    pub usb_sts: UsbSts,

    /// Page Size Register (PAGESIZE) - 32 bits
    pub page_size: u32,

    /// Reserved - 8 bytes
    _reserved1: [u32; 2],

    /// Device Notification Control Register (DNCTRL) - 32 bits
    pub device_notification_ctrl: u32,

    /// Command Ring Control Register (CRCR) - 64 bits
    pub command_ring_ctrl: CommandRingControl,

    /// Reserved - 16 bytes
    _reserved2: [u32; 4],

    /// Device Context Base Address Array Pointer (DCBAAP) - 64 bits
    pub device_context_base_addr: u64,

    /// Configure Register (CONFIG) - 32 bits
    pub config: Config,
}

/// USB Command Register bits
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct UsbCmd(pub u32);

impl UsbCmd {
    /// Run/Stop bit
    pub fn run_stop(&self) -> bool {
        (self.0 & 0x1) != 0
    }

    pub fn set_run_stop(&mut self, value: bool) {
        if value {
            self.0 |= 0x1;
        } else {
            self.0 &= !0x1;
        }
    }

    /// Host Controller Reset
    pub fn hc_reset(&self) -> bool {
        (self.0 & 0x2) != 0
    }

    pub fn set_hc_reset(&mut self, value: bool) {
        if value {
            self.0 |= 0x2;
        } else {
            self.0 &= !0x2;
        }
    }

    /// Interrupter Enable
    pub fn interrupter_enable(&self) -> bool {
        (self.0 & 0x4) != 0
    }

    pub fn set_interrupter_enable(&mut self, value: bool) {
        if value {
            self.0 |= 0x4;
        } else {
            self.0 &= !0x4;
        }
    }

    /// Host System Error Enable
    pub fn host_system_error_enable(&self) -> bool {
        (self.0 & 0x8) != 0
    }

    pub fn set_host_system_error_enable(&mut self, value: bool) {
        if value {
            self.0 |= 0x8;
        } else {
            self.0 &= !0x8;
        }
    }

    /// Light Host Controller Reset
    pub fn light_hc_reset(&self) -> bool {
        (self.0 & 0x80) != 0
    }

    pub fn set_light_hc_reset(&mut self, value: bool) {
        if value {
            self.0 |= 0x80;
        } else {
            self.0 &= !0x80;
        }
    }

    /// Controller Save State
    pub fn controller_save_state(&self) -> bool {
        (self.0 & 0x100) != 0
    }

    pub fn set_controller_save_state(&mut self, value: bool) {
        if value {
            self.0 |= 0x100;
        } else {
            self.0 &= !0x100;
        }
    }

    /// Controller Restore State
    pub fn controller_restore_state(&self) -> bool {
        (self.0 & 0x200) != 0
    }

    pub fn set_controller_restore_state(&mut self, value: bool) {
        if value {
            self.0 |= 0x200;
        } else {
            self.0 &= !0x200;
        }
    }

    /// Enable Wrap Event
    pub fn enable_wrap_event(&self) -> bool {
        (self.0 & 0x400) != 0
    }

    pub fn set_enable_wrap_event(&mut self, value: bool) {
        if value {
            self.0 |= 0x400;
        } else {
            self.0 &= !0x400;
        }
    }

    /// Enable U3 MFINDEX Stop
    pub fn enable_u3_mfindex_stop(&self) -> bool {
        (self.0 & 0x800) != 0
    }

    pub fn set_enable_u3_mfindex_stop(&mut self, value: bool) {
        if value {
            self.0 |= 0x800;
        } else {
            self.0 &= !0x800;
        }
    }
}

/// USB Status Register bits
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct UsbSts(pub u32);

impl UsbSts {
    /// HCHalted - Host Controller Halted
    pub fn hc_halted(&self) -> bool {
        (self.0 & 0x1) != 0
    }

    /// Host System Error
    pub fn host_system_error(&self) -> bool {
        (self.0 & 0x4) != 0
    }

    /// Event Interrupt
    pub fn event_interrupt(&self) -> bool {
        (self.0 & 0x8) != 0
    }

    /// Port Change Detect
    pub fn port_change_detect(&self) -> bool {
        (self.0 & 0x10) != 0
    }

    /// Save State Status
    pub fn save_state_status(&self) -> bool {
        (self.0 & 0x100) != 0
    }

    /// Restore State Status
    pub fn restore_state_status(&self) -> bool {
        (self.0 & 0x200) != 0
    }

    /// Save/Restore Error
    pub fn save_restore_error(&self) -> bool {
        (self.0 & 0x400) != 0
    }

    /// Controller Not Ready
    pub fn controller_not_ready(&self) -> bool {
        (self.0 & 0x800) != 0
    }

    /// Host Controller Error
    pub fn hc_error(&self) -> bool {
        (self.0 & 0x1000) != 0
    }

    /// Clear event interrupt (write 1 to clear)
    pub fn clear_event_interrupt(&mut self) {
        self.0 |= 0x8;
    }

    /// Clear port change detect (write 1 to clear)
    pub fn clear_port_change_detect(&mut self) {
        self.0 |= 0x10;
    }
}

/// Command Ring Control Register (CRCR)
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct CommandRingControl(pub u64);

impl CommandRingControl {
    /// Ring Cycle State (RCS) - bit 0
    /// Indicates the current cycle bit value for the command ring
    pub fn ring_cycle_state(&self) -> bool {
        (self.0 & 0x1) != 0
    }

    pub fn set_ring_cycle_state(&mut self, value: bool) {
        if value {
            self.0 |= 0x1;
        } else {
            self.0 &= !0x1;
        }
    }

    /// Command Stop (CS) - bit 1
    /// Software sets this bit to stop the command ring
    pub fn command_stop(&self) -> bool {
        (self.0 & 0x2) != 0
    }

    pub fn set_command_stop(&mut self, value: bool) {
        if value {
            self.0 |= 0x2;
        } else {
            self.0 &= !0x2;
        }
    }

    /// Command Abort (CA) - bit 2
    /// Software sets this bit to abort the currently executing command
    pub fn command_abort(&self) -> bool {
        (self.0 & 0x4) != 0
    }

    pub fn set_command_abort(&mut self, value: bool) {
        if value {
            self.0 |= 0x4;
        } else {
            self.0 &= !0x4;
        }
    }

    /// Command Ring Running (CRR) - bit 3 (read-only)
    /// Indicates whether the command ring is running
    pub fn command_ring_running(&self) -> bool {
        (self.0 & 0x8) != 0
    }

    /// Reserved bits 4-5 should be preserved

    /// Command Ring Pointer - bits 6-63
    /// Physical address of the command ring (must be 64-byte aligned)
    pub fn command_ring_pointer(&self) -> u64 {
        self.0 & !0x3F  // Clear lower 6 bits
    }

    pub fn set_command_ring_pointer(&mut self, addr: u64) {
        // Ensure address is 64-byte aligned
        assert_eq!(addr & 0x3F, 0, "Command ring pointer must be 64-byte aligned");

        // Preserve lower 6 control bits, set upper address bits
        self.0 = (self.0 & 0x3F) | (addr & !0x3F);
    }

    /// Create a new CRCR with the given ring pointer and cycle state
    pub fn new(ring_pointer: u64, cycle_state: bool) -> Self {
        assert_eq!(ring_pointer & 0x3F, 0, "Command ring pointer must be 64-byte aligned");

        let mut crcr = Self(ring_pointer);
        crcr.set_ring_cycle_state(cycle_state);
        crcr
    }
}

/// Configure Register
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Config(pub u32);

impl Config {
    /// Maximum Device Slots Enabled
    pub fn max_device_slots_enabled(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    pub fn set_max_device_slots_enabled(&mut self, value: u8) {
        self.0 = (self.0 & !0xFF) | (value as u32);
    }

    /// U3 Entry Enable
    pub fn u3_entry_enable(&self) -> bool {
        (self.0 & 0x100) != 0
    }

    pub fn set_u3_entry_enable(&mut self, value: bool) {
        if value {
            self.0 |= 0x100;
        } else {
            self.0 &= !0x100;
        }
    }

    /// Configuration Information Enable
    pub fn config_info_enable(&self) -> bool {
        (self.0 & 0x200) != 0
    }

    pub fn set_config_info_enable(&mut self, value: bool) {
        if value {
            self.0 |= 0x200;
        } else {
            self.0 &= !0x200;
        }
    }
}

/// Port Status and Control Register (PORTSC)
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct PortSc(pub u32);

impl PortSc {
    /// Current Connect Status
    pub fn current_connect_status(&self) -> bool {
        (self.0 & 0x1) != 0
    }

    /// Port Enabled/Disabled
    pub fn port_enabled(&self) -> bool {
        (self.0 & 0x2) != 0
    }

    pub fn set_port_enabled(&mut self, value: bool) {
        if value {
            self.0 |= 0x2;
        } else {
            self.0 &= !0x2;
        }
    }

    /// Over-current Active
    pub fn over_current_active(&self) -> bool {
        (self.0 & 0x8) != 0
    }

    /// Port Reset
    pub fn port_reset(&self) -> bool {
        (self.0 & 0x10) != 0
    }

    pub fn set_port_reset(&mut self, value: bool) {
        if value {
            self.0 |= 0x10;
        } else {
            self.0 &= !0x10;
        }
    }

    /// Port Link State
    pub fn port_link_state(&self) -> u8 {
        ((self.0 >> 5) & 0xF) as u8
    }

    pub fn set_port_link_state(&mut self, value: u8) {
        self.0 = (self.0 & !(0xF << 5)) | ((value as u32 & 0xF) << 5);
    }

    /// Port Power
    pub fn port_power(&self) -> bool {
        (self.0 & 0x200) != 0
    }

    pub fn set_port_power(&mut self, value: bool) {
        if value {
            self.0 |= 0x200;
        } else {
            self.0 &= !0x200;
        }
    }

    /// Port Speed
    pub fn port_speed(&self) -> u8 {
        ((self.0 >> 10) & 0xF) as u8
    }

    /// Port Indicator Control
    pub fn port_indicator(&self) -> u8 {
        ((self.0 >> 14) & 0x3) as u8
    }

    pub fn set_port_indicator(&mut self, value: u8) {
        self.0 = (self.0 & !(0x3 << 14)) | ((value as u32 & 0x3) << 14);
    }

    /// Port Link State Write Strobe
    pub fn port_link_state_write_strobe(&self) -> bool {
        (self.0 & 0x10000) != 0
    }

    pub fn set_port_link_state_write_strobe(&mut self, value: bool) {
        if value {
            self.0 |= 0x10000;
        } else {
            self.0 &= !0x10000;
        }
    }

    /// Connect Status Change
    pub fn connect_status_change(&self) -> bool {
        (self.0 & 0x20000) != 0
    }

    /// Clear Connect Status Change (write 1 to clear)
    pub fn clear_connect_status_change(&mut self) {
        self.0 |= 0x20000;
    }

    /// Port Enabled/Disabled Change
    pub fn port_enabled_change(&self) -> bool {
        (self.0 & 0x40000) != 0
    }

    /// Clear Port Enabled Change (write 1 to clear)
    pub fn clear_port_enabled_change(&mut self) {
        self.0 |= 0x40000;
    }

    /// Warm Port Reset Change
    pub fn warm_port_reset_change(&self) -> bool {
        (self.0 & 0x80000) != 0
    }

    /// Clear Warm Port Reset Change (write 1 to clear)
    pub fn clear_warm_port_reset_change(&mut self) {
        self.0 |= 0x80000;
    }

    /// Over-current Change
    pub fn over_current_change(&self) -> bool {
        (self.0 & 0x100000) != 0
    }

    /// Clear Over-current Change (write 1 to clear)
    pub fn clear_over_current_change(&mut self) {
        self.0 |= 0x100000;
    }

    /// Port Reset Change
    pub fn port_reset_change(&self) -> bool {
        (self.0 & 0x200000) != 0
    }

    /// Clear Port Reset Change (write 1 to clear)
    pub fn clear_port_reset_change(&mut self) {
        self.0 |= 0x200000;
    }

    /// Port Link State Change
    pub fn port_link_state_change(&self) -> bool {
        (self.0 & 0x400000) != 0
    }

    /// Clear Port Link State Change (write 1 to clear)
    pub fn clear_port_link_state_change(&mut self) {
        self.0 |= 0x400000;
    }

    /// Port Config Error Change
    pub fn port_config_error_change(&self) -> bool {
        (self.0 & 0x800000) != 0
    }

    /// Clear Port Config Error Change (write 1 to clear)
    pub fn clear_port_config_error_change(&mut self) {
        self.0 |= 0x800000;
    }

    /// Cold Attach Status
    pub fn cold_attach_status(&self) -> bool {
        (self.0 & 0x1000000) != 0
    }

    /// Wake on Connect Enable
    pub fn wake_on_connect_enable(&self) -> bool {
        (self.0 & 0x2000000) != 0
    }

    pub fn set_wake_on_connect_enable(&mut self, value: bool) {
        if value {
            self.0 |= 0x2000000;
        } else {
            self.0 &= !0x2000000;
        }
    }

    /// Wake on Disconnect Enable
    pub fn wake_on_disconnect_enable(&self) -> bool {
        (self.0 & 0x4000000) != 0
    }

    pub fn set_wake_on_disconnect_enable(&mut self, value: bool) {
        if value {
            self.0 |= 0x4000000;
        } else {
            self.0 &= !0x4000000;
        }
    }

    /// Wake on Over-current Enable
    pub fn wake_on_over_current_enable(&self) -> bool {
        (self.0 & 0x8000000) != 0
    }

    pub fn set_wake_on_over_current_enable(&mut self, value: bool) {
        if value {
            self.0 |= 0x8000000;
        } else {
            self.0 &= !0x8000000;
        }
    }

    /// Device Removable
    pub fn device_removable(&self) -> bool {
        (self.0 & 0x40000000) != 0
    }

    /// Warm Port Reset
    pub fn warm_port_reset(&self) -> bool {
        (self.0 & 0x80000000) != 0
    }

    pub fn set_warm_port_reset(&mut self, value: bool) {
        if value {
            self.0 |= 0x80000000;
        } else {
            self.0 &= !0x80000000;
        }
    }
}

/// Runtime Registers
#[repr(C)]
pub struct RuntimeRegisters {
    /// Microframe Index Register (MFINDEX) - 32 bits
    pub mfindex: u32,

    /// Reserved - 28 bytes
    _reserved: [u32; 7],

    /// Interrupter Register Sets (up to 1023 interrupters)
    /// Each interrupter has 8 32-bit registers (32 bytes total)
    pub interrupters: [InterrupterRegisterSet; 1],
}

/// Interrupter Register Set
#[repr(C)]
pub struct InterrupterRegisterSet {
    /// Interrupter Management Register (IMAN) - 32 bits
    pub iman: InterrupterManagement,

    /// Interrupter Moderation Register (IMOD) - 32 bits
    pub imod: InterrupterModeration,

    /// Event Ring Segment Table Size Register (ERSTSZ) - 32 bits
    pub erstsz: u32,

    /// Reserved - 4 bytes
    _reserved: u32,

    /// Event Ring Segment Table Base Address Register (ERSTBA) - 64 bits
    pub erstba: u64,

    /// Event Ring Dequeue Pointer Register (ERDP) - 64 bits
    pub erdp: u64,
}

/// Interrupter Management Register
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct InterrupterManagement(pub u32);

impl InterrupterManagement {
    /// Interrupt Pending
    pub fn interrupt_pending(&self) -> bool {
        (self.0 & 0x1) != 0
    }

    /// Clear Interrupt Pending (write 1 to clear)
    pub fn clear_interrupt_pending(&mut self) {
        self.0 |= 0x1;
    }

    /// Interrupt Enable
    pub fn interrupt_enable(&self) -> bool {
        (self.0 & 0x2) != 0
    }

    pub fn set_interrupt_enable(&mut self, value: bool) {
        if value {
            self.0 |= 0x2;
        } else {
            self.0 &= !0x2;
        }
    }
}

/// Interrupter Moderation Register
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct InterrupterModeration(pub u32);

impl InterrupterModeration {
    /// Interrupt Moderation Interval
    pub fn interrupt_moderation_interval(&self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    pub fn set_interrupt_moderation_interval(&mut self, value: u16) {
        self.0 = (self.0 & !0xFFFF) | (value as u32);
    }

    /// Interrupt Moderation Counter
    pub fn interrupt_moderation_counter(&self) -> u16 {
        ((self.0 >> 16) & 0xFFFF) as u16
    }

    pub fn set_interrupt_moderation_counter(&mut self, value: u16) {
        self.0 = (self.0 & !0xFFFF0000) | ((value as u32) << 16);
    }
}

/// xHCI Register Access Structure
/// Provides safe access to all xHCI MMIO registers
pub struct XhciRegisters {
    /// Base virtual address of the xHCI MMIO region
    base_addr: VirtAddr,

    /// Capability registers (read-only)
    capability_regs: &'static CapabilityRegisters,

    /// Operational registers pointer
    operational_regs: *mut OperationalRegisters,

    /// Runtime registers pointer
    runtime_regs: *mut RuntimeRegisters,

    /// Doorbell array base
    doorbell_base: VirtAddr,
}

unsafe impl Send for XhciRegisters {}

impl XhciRegisters {
    /// Create a new xHCI register accessor from a mapped MMIO base address
    ///
    /// # Safety
    /// The caller must ensure that:
    /// - `base_addr` points to a valid, mapped xHCI MMIO region
    /// - The memory region remains valid for the lifetime of this structure
    /// - No other code accesses the same registers concurrently
    pub unsafe fn new(base_addr: VirtAddr) -> Self {
        unsafe {
            let capability_regs = &*(base_addr.as_ptr::<CapabilityRegisters>());

            let operational_base = base_addr + capability_regs.cap_length as u64;
            let runtime_base = base_addr + capability_regs.runtime_offset as u64;
            let doorbell_base = base_addr + capability_regs.doorbell_offset as u64;

            let operational_regs = operational_base.as_mut_ptr::<OperationalRegisters>();
            let runtime_regs = runtime_base.as_mut_ptr::<RuntimeRegisters>();

            Self {
                base_addr,
                capability_regs,
                operational_regs,
                runtime_regs,
                doorbell_base,
            }
        }
    }

    /// Get the capability registers (read-only)
    pub fn capability(&self) -> &CapabilityRegisters {
        self.capability_regs
    }

    /// Get USB Command register
    pub fn usb_cmd(&self) -> UsbCmd {
        unsafe { read_volatile(&(*self.operational_regs).usb_cmd) }
    }

    /// Set USB Command register
    pub fn set_usb_cmd(&self, cmd: UsbCmd) {
        unsafe {
            write_volatile(&mut (*self.operational_regs).usb_cmd, cmd);
        }
    }

    /// Get USB Status register
    pub fn usb_sts(&self) -> UsbSts {
        unsafe { read_volatile(&(*self.operational_regs).usb_sts) }
    }

    /// Set USB Status register (for clearing status bits)
    pub fn set_usb_sts(&self, sts: UsbSts) {
        unsafe {
            write_volatile(&mut (*self.operational_regs).usb_sts, sts);
        }
    }

    /// Get Page Size register
    pub fn page_size(&self) -> u32 {
        unsafe { read_volatile(&(*self.operational_regs).page_size) }
    }

    /// Get Device Notification Control register
    pub fn device_notification_ctrl(&self) -> u32 {
        unsafe { read_volatile(&(*self.operational_regs).device_notification_ctrl) }
    }

    /// Set Device Notification Control register
    pub fn set_device_notification_ctrl(&self, value: u32) {
        unsafe {
            write_volatile(&mut (*self.operational_regs).device_notification_ctrl, value);
        }
    }

    /// Get Command Ring Control register
    pub fn command_ring_ctrl(&self) -> CommandRingControl {
        unsafe { read_volatile(&(*self.operational_regs).command_ring_ctrl) }
    }

    /// Set Command Ring Control register
    pub fn set_command_ring_ctrl(&self, value: CommandRingControl) {
        unsafe {
            write_volatile(&mut (*self.operational_regs).command_ring_ctrl, value);
        }
    }

    /// Get Device Context Base Address Array Pointer
    pub fn device_context_base_addr(&self) -> u64 {
        unsafe { read_volatile(&(*self.operational_regs).device_context_base_addr) }
    }

    /// Set Device Context Base Address Array Pointer
    pub fn set_device_context_base_addr(&self, value: u64) {
        unsafe {
            write_volatile(&mut (*self.operational_regs).device_context_base_addr, value);
        }
    }

    /// Get Configure register
    pub fn config(&self) -> Config {
        unsafe { read_volatile(&(*self.operational_regs).config) }
    }

    /// Set Configure register
    pub fn set_config(&self, config: Config) {
        unsafe {
            write_volatile(&mut (*self.operational_regs).config, config);
        }
    }

    /// Get Port Status and Control register for a specific port (1-based)
    pub fn port_sc(&self, port: u8) -> PortSc {
        assert!(
            port > 0 && port <= self.capability_regs.hcs_params1.max_ports(),
            "Port {port} out of range"
        );
        // Port registers are at offset 0x400 + (port-1) * 0x10 from operational base
        // Since they're not in the main struct, we need offset-based access
        let operational_base = self.base_addr + self.capability_regs.cap_length as u64;
        let offset = 0x400 + ((port - 1) as u16 * 0x10);
        let addr = operational_base.as_u64() + offset as u64;
        unsafe { PortSc(read_volatile(addr as *const u32)) }
    }

    /// Set Port Status and Control register for a specific port (1-based)
    pub fn set_port_sc(&self, port: u8, portsc: PortSc) {
        assert!(
            port > 0 && port <= self.capability_regs.hcs_params1.max_ports(),
            "Port {port} out of range"
        );
        // Port registers are at offset 0x400 + (port-1) * 0x10 from operational base
        let operational_base = self.base_addr + self.capability_regs.cap_length as u64;
        let offset = 0x400 + ((port - 1) as u16 * 0x10);
        let addr = operational_base.as_u64() + offset as u64;
        unsafe { write_volatile(addr as *mut u32, portsc.0) }
    }

    /// Get Microframe Index register
    pub fn mfindex(&self) -> u32 {
        unsafe { read_volatile(&(*self.runtime_regs).mfindex) }
    }

    /// Get Interrupter Management register for a specific interrupter
    pub fn interrupter_management(&self, interrupter: u16) -> InterrupterManagement {
        assert!(
            interrupter < self.capability_regs.hcs_params1.max_interrupters(),
            "Interrupter {interrupter} out of range"
        );
        // For now, only support interrupter 0 since RuntimeRegisters only has 1 interrupter
        assert_eq!(interrupter, 0, "Only interrupter 0 is currently supported");
        unsafe { read_volatile(&(*self.runtime_regs).interrupters[0].iman) }
    }

    /// Set Interrupter Management register for a specific interrupter
    pub fn set_interrupter_management(&self, interrupter: u16, iman: InterrupterManagement) {
        assert!(
            interrupter < self.capability_regs.hcs_params1.max_interrupters(),
            "Interrupter {interrupter} out of range"
        );
        // For now, only support interrupter 0 since RuntimeRegisters only has 1 interrupter
        assert_eq!(interrupter, 0, "Only interrupter 0 is currently supported");
        unsafe {
            write_volatile(&mut (*self.runtime_regs).interrupters[0].iman, iman);
        }
    }

    /// Ring doorbell for a specific slot/endpoint
    pub fn ring_doorbell(&self, slot_id: u8, endpoint: u8, stream_id: u16) {
        let doorbell_offset = slot_id as u64 * 4;
        let doorbell_value = (stream_id as u32) << 16 | endpoint as u32;
        let addr = self.doorbell_base.as_u64() + doorbell_offset;
        unsafe { write_volatile(addr as *mut u32, doorbell_value) }
    }

    /// Ring host controller doorbell (slot 0)
    pub fn ring_hc_doorbell(&self, command: u8) {
        self.ring_doorbell(0, command, 0);
    }
}

/// Port register offsets (still needed since ports are variable-length arrays)
pub mod port_offsets {
    pub const PORTSC_BASE: u16 = 0x400;
    pub const PORT_REGISTER_SIZE: u16 = 0x10;
}
