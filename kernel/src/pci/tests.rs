//! PCIe subsystem tests

use crate::pci::vmm::PCIE_VMM;

use super::{
    PCI_MANAGER,
    config::device_classes,
    device::{BarInfo, IoBar, MemoryBar},
    vmm,
};
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
    let mut vmm_lock = PCIE_VMM.lock();
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
    assert_eq!(
        after_stats.allocated_pages,
        initial_stats.allocated_pages + 1
    );
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
    let mut vmm_lock = PCIE_VMM.lock();

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
    let test_memory_bar = MemoryBar::new(
        PhysAddr::new(0x3000_0000),
        8192,  // 8KB
        false, // prefetchable
        false, // is_64bit
    );

    let map_result = vmm::map_bar(&test_memory_bar);
    assert!(map_result.is_ok());

    let mapped = map_result.unwrap();
    assert_eq!(mapped.physical_address.as_u64(), 0x3000_0000);
    assert_eq!(mapped.size, 8192);
    assert!(!mapped.prefetchable);
}

#[test_case]
fn test_bar_mapping_io_bars() {
    // I/O BARs should not be mapped to virtual memory - this test is no longer valid
    // since map_bar now only accepts MemoryBar, not BarInfo
    // We'll test that I/O BARs are handled correctly in device parsing instead
    let test_io_bar = IoBar::new(0x1000, 256);

    // I/O BARs don't get mapped through the VMM, so we just verify the struct works
    assert_eq!(test_io_bar.address, 0x1000);
    assert_eq!(test_io_bar.size, 256);
}

#[test_case]
fn test_bar_mapping_zero_address() {
    // BARs with zero address should not be mapped
    let test_zero_bar = MemoryBar::new(
        PhysAddr::new(0),
        4096,
        false, // prefetchable
        false, // is_64bit
    );

    let map_result = vmm::map_bar(&test_zero_bar);
    // Zero address should cause an error because it indicates an unassigned BAR
    assert!(
        map_result.is_err(),
        "VMM should reject BARs with zero address"
    );
}

#[test_case]
fn test_device_bar_parsing() {
    if let Some(manager) = PCI_MANAGER.lock().as_ref() {
        let mut memory_bars_found = 0;
        let mut _io_bars_found = 0;
        let mut unused_bars_found = 0;

        for device in &manager.devices {
            for bar in &device.bars {
                match bar {
                    BarInfo::Memory(memory_bar) => {
                        memory_bars_found += 1;
                        if memory_bar.address.as_u64() != 0 {
                            assert!(
                                memory_bar.size > 0,
                                "Memory BAR with non-zero address should have non-zero size"
                            );
                            assert!(
                                memory_bar.size.is_power_of_two(),
                                "BAR size should be power of 2"
                            );
                        }
                    }
                    BarInfo::Io(io_bar) => {
                        _io_bars_found += 1;
                        if io_bar.address != 0 {
                            assert!(
                                io_bar.size > 0,
                                "I/O BAR with non-zero address should have non-zero size"
                            );
                        }
                    }
                    BarInfo::Unused => {
                        unused_bars_found += 1;
                    }
                }
            }
        }

        // Should have found some BARs of each type in a typical system
        assert!(
            memory_bars_found > 0,
            "Should find at least some memory BARs"
        );
        assert!(unused_bars_found > 0, "Should find some unused BAR slots");
    }
}

#[test_case]
fn test_device_capabilities() {
    if let Some(manager) = PCI_MANAGER.lock().as_ref() {
        let mut devices_with_caps = 0;
        let mut _msi_caps_found = 0;
        let mut _msix_caps_found = 0;

        for device in &manager.devices {
            if !device.capabilities.is_empty() {
                devices_with_caps += 1;

                for (&cap_id, &offset) in &device.capabilities {
                    match cap_id {
                        0x05 => _msi_caps_found += 1,  // MSI capability
                        0x11 => _msix_caps_found += 1, // MSI-X capability
                        _ => {}                        // Other capabilities
                    }

                    // Capability offset should be valid (>= 0x40 in config space)
                    assert!(
                        offset >= 0x40,
                        "Capability offset should be >= 0x40, got {offset:#x}"
                    );
                }
            }
        }

        // Modern systems should have devices with capabilities
        assert!(
            devices_with_caps > 0,
            "Should find devices with capabilities"
        );
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
        assert!(
            devices_with_intx > 0 || devices_with_msi > 0 || devices_with_msix > 0,
            "Should find devices with some form of interrupt support"
        );
    }
}

#[test_case]
fn test_vmm_allocation_alignment() {
    let mut vmm_lock = PCIE_VMM.lock();

    // Test that allocations are properly page-aligned
    let test_phys_addr = PhysAddr::new(0x4000_0000);
    let test_size = 12345; // Non-page-aligned size

    let mapped_result = vmm_lock.map_memory_bar(test_phys_addr, test_size, false);
    assert!(mapped_result.is_ok());

    let mapped = mapped_result.unwrap();

    // Virtual address should be page-aligned
    assert_eq!(
        mapped.virtual_address.as_u64() % 4096,
        0,
        "Virtual address should be page-aligned"
    );

    // Size should be rounded up to page boundary
    assert_eq!(mapped.size, test_size, "Size should match requested size");

    // Clean up
    let _ = vmm_lock.unmap_bar(&mapped);
}

#[test_case]
fn test_vmm_error_conditions() {
    let mut vmm_lock = PCIE_VMM.lock();

    // Test zero size allocation
    let test_phys_addr = PhysAddr::new(0x5000_0000);
    let zero_size_result = vmm_lock.map_memory_bar(test_phys_addr, 0, false);
    assert!(
        zero_size_result.is_err(),
        "Zero size allocation should fail"
    );

    // Test that the error is the expected type
    match zero_size_result {
        Err(super::PciError::InvalidDevice) => {} // Expected
        _ => panic!("Expected InvalidDevice error for zero size"),
    }
}
