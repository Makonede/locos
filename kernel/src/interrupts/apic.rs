use acpi::{handler::PhysicalMapping, platform::{interrupt::IoApic, PlatformInfo}, AcpiHandler, AcpiTables, InterruptModel};
use core::ptr::NonNull;
use x86_64::{structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, PhysFrame, Size4KiB}, PhysAddr, VirtAddr};
use x2apic::lapic::{LocalApic, LocalApicBuilder, xapic_base};
use alloc::vec::Vec;

use super::pic::disable_legacy_pics;

const PAGE_SIZE: usize = 0x1000;

const IOAPICS_VIRTUAL_START: u64 = 0xFFFF_F000_0000_0000;
const LAPIC_TIMER_VECTOR: usize = 0x20;
const LAPIC_ERROR_VECTOR: usize = 0x21;
const LAPIC_SPURIOUS_VECTOR: usize = 0xFF;

/// Sets up the Local APIC and enables it using the x2apic crate.
/// 
/// # Safety
/// Must be called after IDT is loaded
pub unsafe fn setup_apic(rsdp_addr: usize, memory_offset: usize, mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) {
    disable_legacy_pics();

    let mut lapic = LocalApicBuilder::new()
        .timer_vector(LAPIC_TIMER_VECTOR)
        .error_vector(LAPIC_ERROR_VECTOR)
        .spurious_vector(LAPIC_SPURIOUS_VECTOR)
        .build()
        .expect("local apic initialization failed");

    unsafe { lapic.enable() };

    // IO apic
    let ioapic_addrs = get_ioapic_addresses(rsdp_addr, memory_offset);
    if ioapic_addrs.is_empty() {
        panic!("No IO APIC found");
    }

    for (virtaddr, &ioapic_mmio) in (IOAPICS_VIRTUAL_START..).step_by(PAGE_SIZE).zip(ioapic_addrs.iter()) {
        let virtaddr = VirtAddr::new(virtaddr);
        let ioapic_mmio = PhysAddr::new(ioapic_mmio as u64);

        // Map the IO APIC MMIO region to the virtual address space
        unsafe { map_ioapic(mapper, frame_allocator, ioapic_mmio, virtaddr) };
    }
    
    let ioapic_num = ioapic_addrs.len();
    for i in 0..ioapic_num {
        let ioapic = unsafe { x2apic::ioapic::IoApic::new(
            IOAPICS_VIRTUAL_START + (i * PAGE_SIZE) as u64,
        ) };
    }
}

/// Maps the IO apic memory adress to the virtual address space.
/// 
/// # Safety
/// Fundamentally unsafe due to mapping pages
unsafe fn map_ioapic(mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>, ioapic_mmio: PhysAddr, virtaddr: VirtAddr) {
    unsafe {
        mapper.map_to( 
            Page::containing_address(virtaddr),
            PhysFrame::containing_address(ioapic_mmio),
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE | PageTableFlags::NO_EXECUTE,
            frame_allocator,
        ).expect("failed to map io apic").flush();
    }
}
/// Minimal handler for ACPI physical memory mapping.
#[derive(Clone, Copy)]
pub struct KernelAcpiHandler{
    memory_offset: usize,
}

impl KernelAcpiHandler {
    pub fn new(memory_offset: usize) -> Self {
        KernelAcpiHandler { memory_offset }
    }
}

impl AcpiHandler for KernelAcpiHandler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        unsafe { PhysicalMapping::new(
            physical_address,
            NonNull::new((physical_address + self.memory_offset) as *mut T).unwrap(),
            size,
            size,
            *self,
        ) }
    }
    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {}
}

/// Example function to get IO APIC physical addresses using ACPI.
/// Pass the RSDP physical address as `rsdp_addr`.
pub fn get_ioapic_addresses(rsdp_addr: usize, memory_offset: usize) -> Vec<u32> {
    let tables = unsafe { AcpiTables::from_rsdp(KernelAcpiHandler::new(memory_offset), rsdp_addr).unwrap() };
    let platform_info = tables.platform_info().unwrap();

    let mut ioapic_addrs = Vec::new();
    if let InterruptModel::Apic(apic) = &platform_info.interrupt_model {
        for ioapic in apic.io_apics.iter() {
            ioapic_addrs.push(ioapic.address);
        }
    }
    ioapic_addrs
}
