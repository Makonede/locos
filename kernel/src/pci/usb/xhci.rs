//! xHCI (eXtensible Host Controller Interface) driver for USB 3.0.
//!
//! Provides xHCI controller initialization and management.

use alloc::vec::Vec;
use spin::Mutex;

use super::{xhci_registers::XhciRegisters, init_helpers::{init_dcbaa, init_command_ring}};
use crate::{
    info,
    pci::{
        PCI_MANAGER,
        device::{BarInfo, PciDevice},
        vmm::map_bar,
    },
};

/// Global xHCI registers instance
pub static XHCI_REGS: Mutex<Option<XhciRegisters>> = Mutex::new(None);

/// Find all xHCI devices in the system
#[allow(clippy::let_and_return)]
pub fn find_xhci_devices() -> Vec<PciDevice> {
    let lock = PCI_MANAGER.lock();
    let manager = lock.as_ref().unwrap();

    let xhci_devices: Vec<PciDevice> = manager
        .devices
        .iter()
        .filter(|d| d.class_code == 0x0C && d.subclass == 0x03 && d.prog_if == 0x30)
        .cloned()
        .collect();

    info!("Found {} XHCI devices", xhci_devices.len());

    xhci_devices
}

/// Initialize the xHCI controller
///
/// Finds xHCI devices, resets the controller, and allocates the DCBAA.
/// Populates the XHCI_REGS static at the end.
pub fn xhci_init() {
    let devices = find_xhci_devices();
    let Some(primary_device) = devices.first() else {
        info!("No XHCI devices found");
        return;
    };

    assert!(
        primary_device.supports_msix(),
        "XHCI device does not support MSI-X"
    );

    let memory_bar = &primary_device
        .bars
        .iter()
        .find_map(|bar| {
            if let BarInfo::Memory(memory_bar) = bar {
                Some(memory_bar)
            } else {
                None
            }
        })
        .unwrap();

    let mapped_bar = map_bar(memory_bar).unwrap();

    // Create xHCI register accessor
    let mut xhci_regs = unsafe { XhciRegisters::new(mapped_bar.virtual_address) };

    info!("xHCI Controller Information:");
    info!("  HCI Version: {:#x}", xhci_regs.capability().hci_version);
    info!(
        "  Max Device Slots: {}",
        xhci_regs.capability().hcs_params1.max_device_slots()
    );
    info!(
        "  Max Interrupters: {}",
        xhci_regs.capability().hcs_params1.max_interrupters()
    );
    info!(
        "  Max Ports: {}",
        xhci_regs.capability().hcs_params1.max_ports()
    );
    info!(
        "  64-bit Addressing: {}",
        xhci_regs.capability().hcc_params1.ac64()
    );
    info!(
        "  Context Size: {} bytes",
        if xhci_regs.capability().hcc_params1.csz() {
            64
        } else {
            32
        }
    );

    // Check if controller is halted
    let usb_sts = xhci_regs.usb_sts();
    if !usb_sts.hc_halted() {
        info!("Controller is running, stopping it...");
        let mut usb_cmd = xhci_regs.usb_cmd();
        usb_cmd.set_run_stop(false);
        xhci_regs.set_usb_cmd(usb_cmd);

        // Wait for controller to halt
        loop {
            let sts = xhci_regs.usb_sts();
            if sts.hc_halted() {
                break;
            }
        }
        info!("Controller halted");
    } else {
        info!("Controller is already halted");
    }

    info!("Resetting controller...");
    let mut usb_cmd = xhci_regs.usb_cmd();
    usb_cmd.set_hc_reset(true);
    xhci_regs.set_usb_cmd(usb_cmd);

    loop {
        let cmd = xhci_regs.usb_cmd();
        if !cmd.hc_reset() {
            break;
        }
    }

    loop {
        let sts = xhci_regs.usb_sts();
        if !sts.controller_not_ready() {
            break;
        }
    }
    info!("Controller reset complete and ready");

    let max_slots = xhci_regs.capability().hcs_params1.max_device_slots();
    let mut config = xhci_regs.config();
    config.set_max_device_slots_enabled(max_slots);
    xhci_regs.set_config(config);
    info!("Configured {} device slots", max_slots);

    init_dcbaa(&mut xhci_regs);

    init_command_ring(&mut xhci_regs);

    let max_ports = xhci_regs.capability().hcs_params1.max_ports();
    for port in 1..=max_ports {
        let portsc = xhci_regs.port_sc(port);
        if portsc.current_connect_status() {
            info!(
                "Port {}: Device connected (speed: {})",
                port,
                portsc.port_speed()
            );
        } else {
            info!("Port {}: No device connected", port);
        }
    }

    *XHCI_REGS.lock() = Some(xhci_regs);
    info!("xHCI initialization complete");
}
