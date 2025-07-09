use alloc::vec::Vec;

use crate::{info, pci::{device::{BarInfo, PciDevice}, vmm::map_bar, PCI_MANAGER}};

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

pub fn xhci_init() {
    let devices = find_xhci_devices();
    let primary_device = devices.first().expect("No XHCI devices found");

    assert!(primary_device.bars.len() == 1, "XHCI device has more than one BAR");
    assert!(primary_device.supports_msix(), "XHCI device does not support MSI-X");

    if let BarInfo::Memory(memory_bar) = primary_device.bars[0] {
        map_bar(&memory_bar).unwrap();
    } else {
        panic!("XHCI device BAR is not a memory BAR");
    }
}
