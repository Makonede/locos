/*
Copyright © 2024–2025 Mako and JayAndJef

This file is part of locOS.

locOS is free software: you can redistribute it and/or modify it under the terms of the GNU General
Public License as published by the Free Software Foundation, either version 3 of the License, or (at
your option) any later version.

locOS is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the
implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public
License for more details.

You should have received a copy of the GNU General Public License along with locOS. If not, see
<https://www.gnu.org/licenses/>.
*/

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::testing::test_runner)]
#![reexport_test_harness_main = "test_main"]

pub mod gdt;
pub mod interrupts;
pub mod memory;
pub mod meta;
pub mod output;
pub mod pci;
pub mod serial;
pub mod tasks;
pub mod testing;

extern crate alloc;

use core::{arch::asm, panic::PanicInfo};

use alloc::{vec::Vec, format};
use gdt::init_gdt;
use interrupts::{init_idt, setup_apic};
use limine::{
    memory_map::EntryType, request::{
        FramebufferRequest, HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker,
        RsdpRequest, StackSizeRequest,
    }, BaseRevision
};
use memory::{init_frame_allocator, init_heap, init_page_allocator, paging::{self, fill_page_list}};
use output::{flanterm_init, framebuffer::get_info_from_frambuffer};
use x86_64::{VirtAddr, registers::debug};

use crate::{pci::{device::{IoBar, MemoryBar}, usb, PCI_MANAGER}, tasks::scheduler::kexit_task};

#[cfg(not(test))]
use meta::tprint_welcome;
#[cfg(not(test))]
use crate::{interrupts::apic::LAPIC_TIMER_VECTOR, tasks::scheduler::{kcreate_task, kinit_multitasking}};



pub const STACK_SIZE: u64 = 0x100000;

#[unsafe(no_mangle)]
unsafe extern "C" fn kernel_main() -> ! {
    assert!(BASE_REVISION.is_supported());
    init_gdt();
    init_idt();

    let memory_regions = MEMORY_MAP_REQUEST
        .get_response()
        .expect("memory map request failed")
        .entries();

    let physical_memory_offset = HHDM_REQUEST
        .get_response()
        .expect("Hhdm request failed")
        .offset();

    #[allow(unused_variables)]
    for entry in memory_regions {
        debug!(
            "Memory region: base = {:#x} - {:#x}, usable = {:?}",
            entry.base + physical_memory_offset, entry.base + physical_memory_offset + entry.length, entry.entry_type == EntryType::USABLE,
        );
    }

    debug!("Physical memory offset: {:#x}", physical_memory_offset);
    unsafe { fill_page_list(memory_regions, physical_memory_offset as usize) };
    debug!("Filling page list done");
    unsafe { init_frame_allocator(memory_regions, physical_memory_offset) };

    unsafe { paging::init(VirtAddr::new(physical_memory_offset)) };

    unsafe {
        init_heap().expect("heap initialization failed");
    }

    // sum all usable memory regions
    let usable_regions_sum = memory_regions
        .iter()
        .filter(|entry| entry.entry_type == EntryType::USABLE)
        .map(|entry| entry.length)
        .sum::<u64>();

    #[allow(unused_variables)]
    let usable_regions = memory_regions
        .iter()
        .filter(|entry| entry.entry_type == EntryType::USABLE)
        .map(|entry| entry.length)
        .collect::<Vec<_>>();

    debug!(
        "Total usable memory: {} bytes ({:.2} GiB) spread over {:?} regions",
        usable_regions_sum,
        usable_regions_sum as f64 / (1024.0 * 1024.0 * 1024.0),
        usable_regions,
    );
    init_page_allocator(usable_regions_sum);

    let framebuffer_response = FRAMEBUFFER_REQUEST
        .get_response()
        .expect("framebuffer request failed");
    let framebuffer = framebuffer_response
        .framebuffers()
        .next()
        .expect("framebuffer not found");

    if framebuffer.bpp() % 8 != 0 {
        panic!("Framebuffer bpp is not a multiple of 8");
    }

    flanterm_init(
        framebuffer.addr() as *mut u32,
        get_info_from_frambuffer(&framebuffer),
    );

    let rsdp_addr = RSDP_REQUEST
        .get_response()
        .expect("RSDP request failed")
        .address();

    unsafe { setup_apic(rsdp_addr) };

    // Initialize PCIe subsystem
    pci::init_pci(rsdp_addr).expect("failed to initialize PCIe subsystem");

    // List all discovered PCIe devices
    list_pcie_devices();
    usb::init();

    #[cfg(test)]
    {
        // Clear console and run tests before starting kernel tasks
        print!("\x1B[2J\x1B[H"); // Clear screen and move cursor to top
        test_main();
    }

    #[cfg(not(test))]
    {
        kcreate_task(tprint_welcome, "print welcome message");
        //kcreate_task(print_stuff, "print stuff");
        kinit_multitasking();

        x86_64::instructions::interrupts::enable();
        unsafe {
            core::arch::asm!("int {}", const LAPIC_TIMER_VECTOR);
        }
    }

    hcf();
}

pub fn print_stuff() -> ! {
    for i in 0..100 {
        info!("hello from kernel thread 2, iteration {}", i);
    }

    kexit_task();
}

/// List all discovered PCIe devices with detailed information
fn list_pcie_devices() {
    use pci::config::{device_classes, vendor_ids};

    let manager_lock = PCI_MANAGER.lock();

    if let Some(manager) = manager_lock.as_ref() {
        info!("=== PCIe Device Listing ===");
        info!("Total devices found: {}", manager.devices.len());
        info!("");

        // Group devices by class for better organization
        let mut devices_by_class: alloc::collections::BTreeMap<u8, Vec<&pci::device::PciDevice>> =
            alloc::collections::BTreeMap::new();

        for device in &manager.devices {
            devices_by_class.entry(device.class_code).or_default().push(device);
        }

        for (class_code, devices) in devices_by_class {
            let class_name = match class_code {
                device_classes::UNCLASSIFIED => "Unclassified",
                device_classes::MASS_STORAGE => "Mass Storage",
                device_classes::NETWORK => "Network",
                device_classes::DISPLAY => "Display",
                device_classes::MULTIMEDIA => "Multimedia",
                device_classes::MEMORY => "Memory",
                device_classes::BRIDGE => "Bridge",
                device_classes::COMMUNICATION => "Communication",
                device_classes::SYSTEM_PERIPHERAL => "System Peripheral",
                device_classes::INPUT_DEVICE => "Input Device",
                device_classes::DOCKING_STATION => "Docking Station",
                device_classes::PROCESSOR => "Processor",
                device_classes::SERIAL_BUS => "Serial Bus",
                device_classes::WIRELESS => "Wireless",
                device_classes::INTELLIGENT_IO => "Intelligent I/O",
                device_classes::SATELLITE_COMMUNICATION => "Satellite Communication",
                device_classes::ENCRYPTION => "Encryption",
                device_classes::DATA_ACQUISITION => "Data Acquisition",
                device_classes::PROCESSING_ACCELERATOR => "Processing Accelerator",
                device_classes::NON_ESSENTIAL_INSTRUMENTATION => "Non-Essential Instrumentation",
                device_classes::COPROCESSOR => "Coprocessor",
                _ => "Unknown",
            };

            info!("--- {} Devices (Class {:02x}h) ---", class_name, class_code);

            for device in devices {
                let vendor_name = match device.vendor_id {
                    vendor_ids::INTEL => "Intel",
                    vendor_ids::AMD => "AMD",
                    vendor_ids::NVIDIA => "NVIDIA",
                    vendor_ids::BROADCOM => "Broadcom",
                    vendor_ids::QUALCOMM => "Qualcomm",
                    vendor_ids::MARVELL => "Marvell",
                    vendor_ids::REALTEK => "Realtek",
                    vendor_ids::VIA => "VIA",
                    vendor_ids::VMWARE => "VMware",
                    vendor_ids::QEMU => "Legacy QEMU",
                    vendor_ids::REDHAT_QEMU => "QEMU",
                    vendor_ids::REDHAT => "Red Hat",
                    _ => "Unknown",
                };

                info!("  {:02x}:{:02x}.{} [{:04x}:{:04x}] {} - {} (rev {:02x})",
                    device.bus,
                    device.device,
                    device.function,
                    device.vendor_id,
                    device.device_id,
                    vendor_name,
                    device.description(),
                    device.revision_id
                );

                // Show interrupt capabilities
                let mut interrupt_info = Vec::new();
                if device.supports_msix() {
                    interrupt_info.push("MSI-X");
                }
                if device.supports_msi() {
                    interrupt_info.push("MSI");
                }
                if device.interrupt_pin != 0 {
                    interrupt_info.push("INTx");
                }

                if !interrupt_info.is_empty() {
                    info!("    Interrupts: {}", interrupt_info.join(", "));
                }

                // Show BAR assignment status
                let mut bar_status = Vec::new();
                for (i, bar) in device.bars.iter().enumerate() {
                    match bar {
                        pci::device::BarInfo::Memory(MemoryBar { address, size, prefetchable, .. }) => {
                            let assigned = address.as_u64() != 0;
                            let status = if assigned { "ASSIGNED" } else { "UNASSIGNED" };
                            bar_status.push(format!("BAR{}: Memory {:#x} [{}] (size={}KB{})",
                                i, address.as_u64(), status, size >> 10,
                                if *prefetchable { ", prefetchable" } else { "" }));
                        },
                        pci::device::BarInfo::Io(IoBar { address, size }) => {
                            let assigned = *address != 0;
                            let status = if assigned { "ASSIGNED" } else { "UNASSIGNED" };
                            bar_status.push(format!("BAR{i}: I/O {address:#x} [{status}] (size={size}B)"));
                        },
                        pci::device::BarInfo::Unused => {},
                    }
                }

                for bar_info in bar_status {
                    info!("    {}", bar_info);
                }

                // Show capabilities
                for (&cap_id, &offset) in &device.capabilities {
                    info!("    Capability: {:02x}h at offset {:02x}h", cap_id, offset);
                }
            }
            info!("");
        }

        info!("=== End PCIe Device Listing ===");
    } else {
        warn!("PCIe manager not initialized");
    }
}



#[used]
#[unsafe(link_section = ".requests")]
pub static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static STACK_SIZE_REQUEST: StackSizeRequest = StackSizeRequest::new().with_size(STACK_SIZE);

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();
#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("{}", info);
    hcf();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use crate::testing::{exit_qemu, QemuExitCode};

    serial_println!("[failed]");
    serial_println!("Error: {}", info);
    exit_qemu(QemuExitCode::Failed);
    hcf();
}

fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!("hlt");
        }
    }
}


#[test_case]
fn trivial_assertion() {
    let x = 1;
    assert_eq!(1, x);
}

#[test_case]
fn test_basic_arithmetic() {
    let a = 2;
    let b = 2;
    assert_eq!(a + b, 4);

    let c = 10;
    let d = 5;
    assert_eq!(c - d, 5);
}
