//! Helper functions for xHCI initialization.
//!
//! Provides utilities for setting up DMA buffers, command rings, and DCBAA.

use core::{ptr::write_bytes, fmt};

use x86_64::{PhysAddr, VirtAddr};

use crate::{debug, memory::FRAME_ALLOCATOR, pci::usb::xhci_registers::{CommandRingControl, XhciRegisters}};

/// Command ring size in 64-byte TRBs
const COMMAND_RING_SIZE: usize = 256;

/// Initialize the Device Context Base Address Array (DCBAA)
///
/// Should pass in an xHCI registers reference.
pub fn init_dcbaa(xhci_regs: &mut XhciRegisters) {
    let needed_entries = xhci_regs.capability().hcs_params1.max_device_slots() + 1;

    let dcbaa_size = needed_entries as usize * core::mem::size_of::<u64>();
    let frames_needed = dcbaa_size.div_ceil(4096).next_power_of_two();

    let (dcbaa_phys, _) = get_zeroed_dma(frames_needed);

    xhci_regs.set_device_context_base_addr(dcbaa_phys.as_u64());
    debug!("Allocated DCBAA at {:#x} with {} entries", dcbaa_phys, needed_entries);
}

/// Initialize the TRB command ring
///
/// Uses COMMAND_RING_SIZE.
pub fn init_command_ring(xhci_regs: &mut XhciRegisters) {
    let needed_frames = (COMMAND_RING_SIZE * 8).div_ceil(4096).next_power_of_two();
    let (ring_phys, ring_virt) = get_zeroed_dma(needed_frames);

    let first_trb = ring_virt.as_mut_ptr::<Trb>();
    let first_link_trb = unsafe { first_trb.add(COMMAND_RING_SIZE - 1) };
    unsafe {
        (*first_link_trb) = Trb::link(ring_phys.as_u64(), true, false)
    }

    xhci_regs.set_command_ring_ctrl(CommandRingControl::new(ring_phys.as_u64(), true));
    debug!("Allocated command ring at {:#x} with {} TRBs", ring_phys, COMMAND_RING_SIZE);
}


/// Allocate zeroed DMA memory
fn get_zeroed_dma(frames: usize) -> (PhysAddr, VirtAddr) {
    let mut lock = FRAME_ALLOCATOR.lock();
    let allocator = lock.as_mut().unwrap();
    let virt = allocator.allocate_contiguous_pages(frames)
        .expect("Failed to allocate frames for DMA");
    unsafe {
        write_bytes(virt.as_mut_ptr::<()>(), 0, frames * 4096);
    }
    (PhysAddr::new(virt.as_u64() - allocator.hddm_offset), virt)
}


/// A single TRB
/// Each TRB is 16 bytes and contains command, event, or transfer information
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct Trb {
    pub data: u64,
    pub status: u32,
    pub control: u32,
}

/// TRB Types as defined in xHCI specification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrbType {
    // Transfer TRBs (1-7)
    Normal = 1,
    SetupStage = 2,
    DataStage = 3,
    StatusStage = 4,
    Isoch = 5,
    Link = 6,
    EventData = 7,

    // Command TRBs (9-23)
    EnableSlot = 9,
    DisableSlot = 10,
    AddressDevice = 11,
    ConfigureEndpoint = 12,
    EvaluateContext = 13,
    ResetEndpoint = 14,
    StopEndpoint = 15,
    SetTrDequeuePointer = 16,
    ResetDevice = 17,
    ForceEvent = 18,
    NegotiateBandwidth = 19,
    SetLatencyToleranceValue = 20,
    GetPortBandwidth = 21,
    ForceHeader = 22,
    NoOpCommand = 23,

    // Event TRBs (32-38)
    TransferEvent = 32,
    CommandCompletionEvent = 33,
    PortStatusChangeEvent = 34,
    BandwidthRequestEvent = 35,
    DoorbellEvent = 36,
    HostControllerEvent = 37,
    DeviceNotificationEvent = 38,
    MfindexWrapEvent = 39,
}

/// Command Completion Codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompletionCode {
    Invalid = 0,
    Success = 1,
    DataBufferError = 2,
    BabbleDetectedError = 3,
    UsbTransactionError = 4,
    TrbError = 5,
    StallError = 6,
    ResourceError = 7,
    BandwidthError = 8,
    NoSlotsAvailableError = 9,
    InvalidStreamTypeError = 10,
    SlotNotEnabledError = 11,
    EndpointNotEnabledError = 12,
    ShortPacket = 13,
    RingUnderrun = 14,
    RingOverrun = 15,
    VfEventRingFullError = 16,
    ParameterError = 17,
    BandwidthOverrunError = 18,
    ContextStateError = 19,
    NoPingResponseError = 20,
    EventRingFullError = 21,
    IncompatibleDeviceError = 22,
    MissedServiceError = 23,
    CommandRingStopped = 24,
    CommandAborted = 25,
    Stopped = 26,
    StoppedLengthInvalid = 27,
    StoppedShortPacket = 28,
    MaxExitLatencyTooLargeError = 29,
    IsochBufferOverrun = 31,
    EventLostError = 32,
    UndefinedError = 33,
    InvalidStreamIdError = 34,
    SecondaryBandwidthError = 35,
    SplitTransactionError = 36,
}

impl Trb {
    /// Create a new TRB with all fields set to zero
    pub const fn new() -> Self {
        Self {
            data: 0,
            status: 0,
            control: 0,
        }
    }

    /// Create a TRB with specified values
    pub const fn with_values(data: u64, status: u32, control: u32) -> Self {
        Self {
            data,
            status,
            control,
        }
    }

    /// Get the TRB type from the control field (bits 10-15)
    pub fn trb_type(&self) -> u8 {
        ((self.control >> 10) & 0x3F) as u8
    }

    /// Set the TRB type in the control field (bits 10-15)
    pub fn set_trb_type(&mut self, trb_type: TrbType) {
        self.control = (self.control & !0xFC00) | ((trb_type as u32) << 10);
    }

    /// Get the cycle bit (bit 0 of control field)
    pub fn cycle_bit(&self) -> bool {
        (self.control & 0x1) != 0
    }

    /// Set the cycle bit (bit 0 of control field)
    pub fn set_cycle_bit(&mut self, cycle: bool) {
        if cycle {
            self.control |= 0x1;
        } else {
            self.control &= !0x1;
        }
    }

    /// Get the interrupt on completion flag (bit 5 of control field)
    pub fn interrupt_on_completion(&self) -> bool {
        (self.control & 0x20) != 0
    }

    /// Set the interrupt on completion flag (bit 5 of control field)
    pub fn set_interrupt_on_completion(&mut self, ioc: bool) {
        if ioc {
            self.control |= 0x20;
        } else {
            self.control &= !0x20;
        }
    }

    /// Get the immediate data flag (bit 6 of control field)
    pub fn immediate_data(&self) -> bool {
        (self.control & 0x40) != 0
    }

    /// Set the immediate data flag (bit 6 of control field)
    pub fn set_immediate_data(&mut self, immediate: bool) {
        if immediate {
            self.control |= 0x40;
        } else {
            self.control &= !0x40;
        }
    }

    /// Get the chain bit (bit 4 of control field)
    pub fn chain_bit(&self) -> bool {
        (self.control & 0x10) != 0
    }

    /// Set the chain bit (bit 4 of control field)
    pub fn set_chain_bit(&mut self, chain: bool) {
        if chain {
            self.control |= 0x10;
        } else {
            self.control &= !0x10;
        }
    }

    /// Get the completion code from status field (bits 24-31)
    pub fn completion_code(&self) -> u8 {
        ((self.status >> 24) & 0xFF) as u8
    }

    /// Set the completion code in status field (bits 24-31)
    pub fn set_completion_code(&mut self, code: CompletionCode) {
        self.status = (self.status & !0xFF000000) | ((code as u32) << 24);
    }

    /// Get the transfer length from status field (bits 0-16)
    pub fn transfer_length(&self) -> u32 {
        self.status & 0x1FFFF
    }

    /// Set the transfer length in status field (bits 0-16)
    pub fn set_transfer_length(&mut self, length: u32) {
        self.status = (self.status & !0x1FFFF) | (length & 0x1FFFF);
    }

    /// Get the slot ID from control field (bits 24-31)
    pub fn slot_id(&self) -> u8 {
        ((self.control >> 24) & 0xFF) as u8
    }

    /// Set the slot ID in control field (bits 24-31)
    pub fn set_slot_id(&mut self, slot_id: u8) {
        self.control = (self.control & !0xFF000000) | ((slot_id as u32) << 24);
    }

    /// Get the endpoint ID from control field (bits 16-20)
    pub fn endpoint_id(&self) -> u8 {
        ((self.control >> 16) & 0x1F) as u8
    }

    /// Set the endpoint ID in control field (bits 16-20)
    pub fn set_endpoint_id(&mut self, endpoint_id: u8) {
        self.control = (self.control & !0x1F0000) | ((endpoint_id as u32) << 16);
    }

    // === Command TRB Creation Methods ===

    /// Create a No-Op Command TRB
    pub fn no_op_command(cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.set_trb_type(TrbType::NoOpCommand);
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create an Enable Slot Command TRB
    pub fn enable_slot_command(slot_type: u8, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.set_trb_type(TrbType::EnableSlot);
        trb.control |= (slot_type as u32) << 16; // Slot Type in bits 16-20
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create a Disable Slot Command TRB
    pub fn disable_slot_command(slot_id: u8, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.set_trb_type(TrbType::DisableSlot);
        trb.set_slot_id(slot_id);
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create an Address Device Command TRB
    pub fn address_device_command(input_context_ptr: u64, slot_id: u8, block_set_address: bool, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.data = input_context_ptr & !0x3F; // Must be 64-byte aligned
        trb.set_trb_type(TrbType::AddressDevice);
        trb.set_slot_id(slot_id);
        if block_set_address {
            trb.control |= 0x200; // BSR bit (bit 9)
        }
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create a Configure Endpoint Command TRB
    pub fn configure_endpoint_command(input_context_ptr: u64, slot_id: u8, deconfigure: bool, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.data = input_context_ptr & !0x3F; // Must be 64-byte aligned
        trb.set_trb_type(TrbType::ConfigureEndpoint);
        trb.set_slot_id(slot_id);
        if deconfigure {
            trb.control |= 0x200; // DC bit (bit 9)
        }
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create a Reset Device Command TRB
    pub fn reset_device_command(slot_id: u8, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.set_trb_type(TrbType::ResetDevice);
        trb.set_slot_id(slot_id);
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create a Stop Endpoint Command TRB
    pub fn stop_endpoint_command(slot_id: u8, endpoint_id: u8, suspend: bool, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.set_trb_type(TrbType::StopEndpoint);
        trb.set_slot_id(slot_id);
        trb.set_endpoint_id(endpoint_id);
        if suspend {
            trb.control |= 0x800000; // SP bit (bit 23)
        }
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create a Reset Endpoint Command TRB
    pub fn reset_endpoint_command(slot_id: u8, endpoint_id: u8, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.set_trb_type(TrbType::ResetEndpoint);
        trb.set_slot_id(slot_id);
        trb.set_endpoint_id(endpoint_id);
        trb.set_cycle_bit(cycle);
        trb
    }

    // === Transfer TRB Creation Methods ===

    /// Create a Normal Transfer TRB
    pub fn normal_transfer(buffer_ptr: u64, length: u32, td_size: u8, interrupt_on_completion: bool, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.data = buffer_ptr;
        trb.set_transfer_length(length);
        trb.status |= (td_size as u32) << 17; // TD Size in bits 17-21
        trb.set_trb_type(TrbType::Normal);
        trb.set_interrupt_on_completion(interrupt_on_completion);
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create a Setup Stage TRB for control transfers
    pub fn setup_stage(setup_data: u64, transfer_length: u32, immediate_data: bool, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.data = setup_data;
        trb.set_transfer_length(transfer_length);
        trb.set_trb_type(TrbType::SetupStage);
        trb.set_immediate_data(immediate_data);
        trb.control |= 0x80; // TRT (Transfer Type) = Setup (bits 16-17 = 00, but we set IDT)
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create a Data Stage TRB for control transfers
    pub fn data_stage(buffer_ptr: u64, length: u32, direction_in: bool, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.data = buffer_ptr;
        trb.set_transfer_length(length);
        trb.set_trb_type(TrbType::DataStage);
        if direction_in {
            trb.control |= 0x10000; // DIR bit (bit 16) = 1 for IN
        }
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create a Status Stage TRB for control transfers
    pub fn status_stage(direction_in: bool, interrupt_on_completion: bool, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.set_trb_type(TrbType::StatusStage);
        if direction_in {
            trb.control |= 0x10000; // DIR bit (bit 16) = 1 for IN
        }
        trb.set_interrupt_on_completion(interrupt_on_completion);
        trb.set_cycle_bit(cycle);
        trb
    }

    /// Create a Link TRB
    pub fn link(ring_segment_ptr: u64, toggle_cycle: bool, cycle: bool) -> Self {
        let mut trb = Self::new();
        trb.data = ring_segment_ptr & !0x3F; // Must be 64-byte aligned
        trb.set_trb_type(TrbType::Link);
        if toggle_cycle {
            trb.control |= 0x2; // TC bit (bit 1)
        }
        trb.set_cycle_bit(cycle);
        trb
    }

    // === Event TRB Parsing Methods ===

    /// Check if this is a Command Completion Event TRB
    pub fn is_command_completion_event(&self) -> bool {
        self.trb_type() == TrbType::CommandCompletionEvent as u8
    }

    /// Check if this is a Transfer Event TRB
    pub fn is_transfer_event(&self) -> bool {
        self.trb_type() == TrbType::TransferEvent as u8
    }

    /// Check if this is a Port Status Change Event TRB
    pub fn is_port_status_change_event(&self) -> bool {
        self.trb_type() == TrbType::PortStatusChangeEvent as u8
    }

    /// Get the command TRB pointer from a Command Completion Event (data field)
    pub fn command_trb_pointer(&self) -> u64 {
        self.data
    }

    /// Get the port ID from a Port Status Change Event (bits 24-31 of data field)
    pub fn port_id(&self) -> u8 {
        ((self.data >> 24) & 0xFF) as u8
    }

    /// Get the TRB pointer from a Transfer Event (data field)
    pub fn trb_pointer(&self) -> u64 {
        self.data
    }

    /// Check if the event indicates success
    pub fn is_success(&self) -> bool {
        self.completion_code() == CompletionCode::Success as u8
    }

    /// Check if the event indicates an error
    pub fn is_error(&self) -> bool {
        let code = self.completion_code();
        code != CompletionCode::Success as u8 && code != CompletionCode::ShortPacket as u8
    }

    /// Get a human-readable description of the completion code
    pub fn completion_code_description(&self) -> &'static str {
        match self.completion_code() {
            1 => "Success",
            2 => "Data Buffer Error",
            3 => "Babble Detected Error",
            4 => "USB Transaction Error",
            5 => "TRB Error",
            6 => "Stall Error",
            7 => "Resource Error",
            8 => "Bandwidth Error",
            9 => "No Slots Available Error",
            10 => "Invalid Stream Type Error",
            11 => "Slot Not Enabled Error",
            12 => "Endpoint Not Enabled Error",
            13 => "Short Packet",
            14 => "Ring Underrun",
            15 => "Ring Overrun",
            17 => "Parameter Error",
            19 => "Context State Error",
            24 => "Command Ring Stopped",
            25 => "Command Aborted",
            26 => "Stopped",
            _ => "Unknown Error",
        }
    }

    /// Check if this TRB is valid (has proper alignment and reasonable values)
    pub fn is_valid(&self) -> bool {
        // Basic sanity checks
        let trb_type = self.trb_type();

        // Check if TRB type is in valid range
        if trb_type == 0 || trb_type == 8 || (trb_type > 23 && trb_type < 32) || trb_type > 39 {
            return false;
        }

        // For TRBs with pointers, check alignment
        match trb_type {
            6 => self.data & 0x3F == 0, // Link TRB - 64-byte aligned
            11 | 12 => self.data & 0x3F == 0, // Address Device, Configure Endpoint - 64-byte aligned
            _ => true,
        }
    }
}

/// TRB size in bytes (always 16 bytes)
pub const TRB_SIZE: usize = 16;

/// Maximum TRBs per ring segment (4KB page / 16 bytes per TRB = 256 TRBs)
pub const MAX_TRBS_PER_SEGMENT: usize = 4096 / TRB_SIZE;

/// Ring segment size in bytes (4KB page)
pub const RING_SEGMENT_SIZE: usize = 4096;

impl fmt::Display for Trb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TRB {{ type: {}, cycle: {}, data: {:#018x}, status: {:#010x}, control: {:#010x} }}",
               self.trb_type(),
               self.cycle_bit(),
               self.data,
               self.status,
               self.control)
    }
}

impl fmt::Display for TrbType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            TrbType::Normal => "Normal",
            TrbType::SetupStage => "Setup Stage",
            TrbType::DataStage => "Data Stage",
            TrbType::StatusStage => "Status Stage",
            TrbType::Isoch => "Isochronous",
            TrbType::Link => "Link",
            TrbType::EventData => "Event Data",
            TrbType::EnableSlot => "Enable Slot",
            TrbType::DisableSlot => "Disable Slot",
            TrbType::AddressDevice => "Address Device",
            TrbType::ConfigureEndpoint => "Configure Endpoint",
            TrbType::EvaluateContext => "Evaluate Context",
            TrbType::ResetEndpoint => "Reset Endpoint",
            TrbType::StopEndpoint => "Stop Endpoint",
            TrbType::SetTrDequeuePointer => "Set TR Dequeue Pointer",
            TrbType::ResetDevice => "Reset Device",
            TrbType::ForceEvent => "Force Event",
            TrbType::NegotiateBandwidth => "Negotiate Bandwidth",
            TrbType::SetLatencyToleranceValue => "Set Latency Tolerance Value",
            TrbType::GetPortBandwidth => "Get Port Bandwidth",
            TrbType::ForceHeader => "Force Header",
            TrbType::NoOpCommand => "No-Op Command",
            TrbType::TransferEvent => "Transfer Event",
            TrbType::CommandCompletionEvent => "Command Completion Event",
            TrbType::PortStatusChangeEvent => "Port Status Change Event",
            TrbType::BandwidthRequestEvent => "Bandwidth Request Event",
            TrbType::DoorbellEvent => "Doorbell Event",
            TrbType::HostControllerEvent => "Host Controller Event",
            TrbType::DeviceNotificationEvent => "Device Notification Event",
            TrbType::MfindexWrapEvent => "Mfindex Wrap Event",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for CompletionCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            CompletionCode::Invalid => "Invalid",
            CompletionCode::Success => "Success",
            CompletionCode::DataBufferError => "Data Buffer Error",
            CompletionCode::BabbleDetectedError => "Babble Detected Error",
            CompletionCode::UsbTransactionError => "USB Transaction Error",
            CompletionCode::TrbError => "TRB Error",
            CompletionCode::StallError => "Stall Error",
            CompletionCode::ResourceError => "Resource Error",
            CompletionCode::BandwidthError => "Bandwidth Error",
            CompletionCode::NoSlotsAvailableError => "No Slots Available Error",
            CompletionCode::InvalidStreamTypeError => "Invalid Stream Type Error",
            CompletionCode::SlotNotEnabledError => "Slot Not Enabled Error",
            CompletionCode::EndpointNotEnabledError => "Endpoint Not Enabled Error",
            CompletionCode::ShortPacket => "Short Packet",
            CompletionCode::RingUnderrun => "Ring Underrun",
            CompletionCode::RingOverrun => "Ring Overrun",
            CompletionCode::VfEventRingFullError => "VF Event Ring Full Error",
            CompletionCode::ParameterError => "Parameter Error",
            CompletionCode::BandwidthOverrunError => "Bandwidth Overrun Error",
            CompletionCode::ContextStateError => "Context State Error",
            CompletionCode::NoPingResponseError => "No Ping Response Error",
            CompletionCode::EventRingFullError => "Event Ring Full Error",
            CompletionCode::IncompatibleDeviceError => "Incompatible Device Error",
            CompletionCode::MissedServiceError => "Missed Service Error",
            CompletionCode::CommandRingStopped => "Command Ring Stopped",
            CompletionCode::CommandAborted => "Command Aborted",
            CompletionCode::Stopped => "Stopped",
            CompletionCode::StoppedLengthInvalid => "Stopped Length Invalid",
            CompletionCode::StoppedShortPacket => "Stopped Short Packet",
            CompletionCode::MaxExitLatencyTooLargeError => "Max Exit Latency Too Large Error",
            CompletionCode::IsochBufferOverrun => "Isochronous Buffer Overrun",
            CompletionCode::EventLostError => "Event Lost Error",
            CompletionCode::UndefinedError => "Undefined Error",
            CompletionCode::InvalidStreamIdError => "Invalid Stream ID Error",
            CompletionCode::SecondaryBandwidthError => "Secondary Bandwidth Error",
            CompletionCode::SplitTransactionError => "Split Transaction Error",
        };
        write!(f, "{name}")
    }
}

impl TrbType {
    /// Check if this TRB type is a command TRB
    pub fn is_command(&self) -> bool {
        matches!(*self as u8, 9..=23)
    }

    /// Check if this TRB type is an event TRB
    pub fn is_event(&self) -> bool {
        matches!(*self as u8, 32..=39)
    }

    /// Check if this TRB type is a transfer TRB
    pub fn is_transfer(&self) -> bool {
        matches!(*self as u8, 1..=7)
    }
}

impl CompletionCode {
    /// Check if this completion code indicates success
    pub fn is_success(&self) -> bool {
        matches!(self, CompletionCode::Success | CompletionCode::ShortPacket)
    }

    /// Check if this completion code indicates an error
    pub fn is_error(&self) -> bool {
        !self.is_success() && *self != CompletionCode::Invalid
    }
}
