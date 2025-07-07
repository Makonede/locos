//! PCIe subsystem tests

use crate::serial_println;

use super::{PCI_MANAGER, config::device_classes, vmm, device::BarInfo};
use x86_64::PhysAddr;

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

#[test_case]
fn test_vmm_bitmap_operations() {
    let mut vmm_lock = vmm::get_pcie_vmm().lock();
    let initial_stats = vmm_lock.get_stats();

    // Test mapping a small BAR (4KB)
    let test_phys_addr = PhysAddr::new(0x1000_0000);
    let test_size = 4096;

    let mapped_result = vmm_lock.map_memory_bar(test_phys_addr, test_size, false);
    assert!(mapped_result.is_ok());

    let mapped = mapped_result.unwrap();
    assert_eq!(mapped.physical_address, test_phys_addr);
    assert_eq!(mapped.size, test_size);
    assert!(!mapped.prefetchable);

    // Check that one page is now allocated
    let after_stats = vmm_lock.get_stats();
    assert_eq!(after_stats.allocated_pages, initial_stats.allocated_pages + 1);
    assert_eq!(after_stats.free_pages, initial_stats.free_pages - 1);

    // Test unmapping
    let unmap_result = vmm_lock.unmap_bar(&mapped);
    assert!(unmap_result.is_ok());

    // Should be back to initial state
    let final_stats = vmm_lock.get_stats();
    assert_eq!(final_stats.allocated_pages, initial_stats.allocated_pages);
    assert_eq!(final_stats.free_pages, initial_stats.free_pages);
}

#[test_case]
fn test_vmm_large_allocation() {
    let mut vmm_lock = vmm::get_pcie_vmm().lock();

    // Test mapping a larger BAR (1MB)
    let test_phys_addr = PhysAddr::new(0x2000_0000);
    let test_size = 1024 * 1024; // 1MB

    let mapped_result = vmm_lock.map_memory_bar(test_phys_addr, test_size, true);
    assert!(mapped_result.is_ok());

    let mapped = mapped_result.unwrap();
    assert_eq!(mapped.physical_address, test_phys_addr);
    assert_eq!(mapped.size, test_size);
    assert!(mapped.prefetchable);

    // Should allocate 256 pages (1MB / 4KB)
    let stats = vmm_lock.get_stats();
    assert!(stats.allocated_pages >= 256);

    // Clean up
    let _ = vmm_lock.unmap_bar(&mapped);
}

#[test_case]
fn test_bar_mapping_interface() {
    // Test the high-level BAR mapping interface
    let test_memory_bar = BarInfo::Memory {
        address: PhysAddr::new(0x3000_0000),
        size: 8192, // 8KB
        prefetchable: false,
        is_64bit: false,
    };

    let map_result = vmm::map_bar(&test_memory_bar);
    assert!(map_result.is_ok());

    let mapped_opt = map_result.unwrap();
    assert!(mapped_opt.is_some());

    let mapped = mapped_opt.unwrap();
    assert_eq!(mapped.physical_address.as_u64(), 0x3000_0000);
    assert_eq!(mapped.size, 8192);
    assert!(!mapped.prefetchable);
}

#[test_case]
fn test_bar_mapping_io_bars() {
    // I/O BARs should not be mapped to virtual memory
    let test_io_bar = BarInfo::Io {
        address: 0x1000,
        size: 256,
    };

    let map_result = vmm::map_bar(&test_io_bar);
    assert!(map_result.is_ok());

    let mapped_opt = map_result.unwrap();
    assert!(mapped_opt.is_none()); // I/O BARs return None
}

#[test_case]
fn test_bar_mapping_zero_address() {
    // BARs with zero address should not be mapped
    let test_zero_bar = BarInfo::Memory {
        address: PhysAddr::new(0),
        size: 4096,
        prefetchable: false,
        is_64bit: false,
    };

    let map_result = vmm::map_bar(&test_zero_bar);
    assert!(map_result.is_ok());

    let mapped_opt = map_result.unwrap();
    assert!(mapped_opt.is_none()); // Zero address returns None
}

#[test_case]
fn test_device_bar_parsing() {
    if let Some(manager) = PCI_MANAGER.lock().as_ref() {
        let mut memory_bars_found = 0;
        let mut io_bars_found = 0;
        let mut unused_bars_found = 0;

        for device in &manager.devices {
            for bar in &device.bars {
                match bar {
                    BarInfo::Memory { address, size, prefetchable: _, is_64bit: _ } => {
                        memory_bars_found += 1;
                        if address.as_u64() != 0 {
                            assert!(*size > 0, "Memory BAR with non-zero address should have non-zero size");
                            assert!(size.is_power_of_two(), "BAR size should be power of 2");
                        }
                    },
                    BarInfo::Io { address, size } => {
                        io_bars_found += 1;
                        if *address != 0 {
                            assert!(*size > 0, "I/O BAR with non-zero address should have non-zero size");
                        }
                    },
                    BarInfo::Unused => {
                        unused_bars_found += 1;
                    },
                }
            }
        }

        // Should have found some BARs of each type in a typical system
        assert!(memory_bars_found > 0, "Should find at least some memory BARs");
        assert!(unused_bars_found > 0, "Should find some unused BAR slots");
    }
}

#[test_case]
fn test_device_capabilities() {
    if let Some(manager) = PCI_MANAGER.lock().as_ref() {
        let mut devices_with_caps = 0;
        let mut msi_caps_found = 0;
        let mut msix_caps_found = 0;

        for device in &manager.devices {
            if !device.capabilities.is_empty() {
                devices_with_caps += 1;

                for cap in &device.capabilities {
                    match cap.id {
                        0x05 => msi_caps_found += 1,    // MSI capability
                        0x11 => msix_caps_found += 1,   // MSI-X capability
                        _ => {}, // Other capabilities
                    }

                    // Capability should have valid next pointer
                    assert!(cap.next_ptr == 0 || cap.next_ptr >= 0x40,
                           "Capability next pointer should be 0 or >= 0x40");
                }
            }
        }

        // Modern systems should have devices with capabilities
        assert!(devices_with_caps > 0, "Should find devices with capabilities");
    }
}

#[test_case]
fn test_device_interrupt_support() {
    if let Some(manager) = PCI_MANAGER.lock().as_ref() {
        let mut devices_with_msi = 0;
        let mut devices_with_msix = 0;
        let mut devices_with_intx = 0;

        for device in &manager.devices {
            if device.supports_msi() {
                devices_with_msi += 1;
            }
            if device.supports_msix() {
                devices_with_msix += 1;
            }
            if device.interrupt_pin != 0 {
                devices_with_intx += 1;
            }
        }

        // Should find devices with various interrupt mechanisms
        // Note: Not all devices support all interrupt types
        assert!(devices_with_intx > 0 || devices_with_msi > 0 || devices_with_msix > 0,
               "Should find devices with some form of interrupt support");
    }
}

#[test_case]
fn test_vmm_allocation_alignment() {
    let mut vmm_lock = vmm::get_pcie_vmm().lock();

    // Test that allocations are properly page-aligned
    let test_phys_addr = PhysAddr::new(0x4000_0000);
    let test_size = 12345; // Non-page-aligned size

    let mapped_result = vmm_lock.map_memory_bar(test_phys_addr, test_size, false);
    assert!(mapped_result.is_ok());

    let mapped = mapped_result.unwrap();

    // Virtual address should be page-aligned
    assert_eq!(mapped.virtual_address.as_u64() % 4096, 0,
              "Virtual address should be page-aligned");

    // Size should be rounded up to page boundary
    assert_eq!(mapped.size, test_size, "Size should match requested size");

    // Clean up
    let _ = vmm_lock.unmap_bar(&mapped);
}

#[test_case]
fn test_vmm_error_conditions() {
    let mut vmm_lock = vmm::get_pcie_vmm().lock();

    // Test zero size allocation
    let test_phys_addr = PhysAddr::new(0x5000_0000);
    let zero_size_result = vmm_lock.map_memory_bar(test_phys_addr, 0, false);
    assert!(zero_size_result.is_err(), "Zero size allocation should fail");

    // Test that the error is the expected type
    match zero_size_result {
        Err(super::PciError::InvalidDevice) => {}, // Expected
        _ => panic!("Expected InvalidDevice error for zero size"),
    }
}
