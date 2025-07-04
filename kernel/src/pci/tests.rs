//! PCIe subsystem tests

use super::{PCI_MANAGER, config::device_classes};

#[test_case]
fn test_pci_manager_initialized() {
    let manager_lock = PCI_MANAGER.lock();
    assert!(manager_lock.is_some());
}

#[test_case]
fn test_ecam_regions_valid() {
    if let Some(manager) = PCI_MANAGER.lock().as_ref() {
        assert!(!manager.ecam_regions.is_empty());

        for region in &manager.ecam_regions {
            assert!(region.start_bus <= region.end_bus);
            assert_ne!(region.base_address.as_u64(), 0);
            assert_ne!(region.virtual_address.as_u64(), 0);
        }
    }
}

#[test_case]
fn test_device_enumeration() {
    if let Some(manager) = PCI_MANAGER.lock().as_ref() {
        assert!(!manager.devices.is_empty());

        for device in &manager.devices {
            assert_ne!(device.vendor_id, 0xFFFF);
            assert!(device.device < 32);
            assert!(device.function < 8);
        }
    }
}

#[test_case]
fn test_device_classification() {
    if let Some(manager) = PCI_MANAGER.lock().as_ref() {
        let bridge_devices = manager.get_devices_by_class(device_classes::BRIDGE);
        assert!(!bridge_devices.is_empty());

        for device in &bridge_devices {
            assert_eq!(device.class_code, device_classes::BRIDGE);
        }
    }
}
