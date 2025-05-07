use acpi::{handler::PhysicalMapping, AcpiHandler, AcpiTables, InterruptModel};
use core::ptr::NonNull;
use x86_64::{registers::model_specific::Msr, structures::{idt::InterruptStackFrame, paging::{FrameAllocator, Mapper, Page, PageTableFlags, PhysFrame, Size4KiB}}, PhysAddr, VirtAddr};
use x2apic::{ioapic::{IrqFlags, IrqMode, RedirectionTableEntry}, lapic::{xapic_base, LocalApicBuilder}};
use alloc::vec::Vec;

use super::{idt::IDT, pic::disable_legacy_pics};

const PAGE_SIZE: usize = 0x1000;
const X2APIC_EOI_MSR: u32 = 0x80B;

const IOAPICS_VIRTUAL_START: u64 = 0xFFFF_F000_0000_0000;
const XAPIC_VIRTUAL_START: u64 = 0xFFFF_F100_0000_0000;
const LAPIC_TIMER_VECTOR: u8 = 0x20;
const LAPIC_ERROR_VECTOR: u8 = 0x21;
const LAPIC_SPURIOUS_VECTOR: u8 = 0xFF;
const IOAPIC_TIMER_VECTOR: u8 = 0x20;
const IOAPIC_TIMER_INPUT: u8 = 0;

/// Sets up the Local APIC and enables it using the x2apic crate.
/// 
/// # Safety
/// Must be called after IDT is loaded
pub unsafe fn setup_apic(rsdp_addr: usize, memory_offset: usize, mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) {
    disable_legacy_pics();

    let mut builder = LocalApicBuilder::new();
    let mut lapic = builder
        .timer_vector(LAPIC_TIMER_VECTOR as usize)
        .error_vector(LAPIC_ERROR_VECTOR as usize)
        .spurious_vector(LAPIC_SPURIOUS_VECTOR as usize);

    match detect_lapic_support() {
        ApicSupport::XApic => {
            let lapic_base = unsafe { xapic_base() };
            map_lapic_registers(mapper, frame_allocator, PhysAddr::new(lapic_base), VirtAddr::new(XAPIC_VIRTUAL_START));
            lapic = lapic.set_xapic_base(XAPIC_VIRTUAL_START);
        }
        ApicSupport::None => {
            panic!("No APIC support detected");
        }
        ApicSupport::X2Apic => (),
    }

    let mut final_lapic = lapic
        .build()
        .unwrap();

    unsafe { final_lapic.enable() };

    // IO apic
    let ioapic_addrs = get_ioapic_info(rsdp_addr, memory_offset);
    if ioapic_addrs.is_empty() {
        panic!("No IO APIC found");
    }

    for (virtaddr, &(ioapic_mmio, _)) in (IOAPICS_VIRTUAL_START..).step_by(PAGE_SIZE).zip(ioapic_addrs.iter()) {
        let virtaddr = VirtAddr::new(virtaddr);
        let ioapic_mmio = PhysAddr::new(ioapic_mmio as u64);

        // Map the IO APIC MMIO region to the virtual address space
        unsafe { map_ioapic(mapper, frame_allocator, ioapic_mmio, virtaddr) };
    }
    
    let mut ioapics = Vec::with_capacity(ioapic_addrs.len());
    for (i, &(_, gsi_base)) in ioapic_addrs.iter().enumerate() {
        ioapics.push((
            unsafe { x2apic::ioapic::IoApic::new(IOAPICS_VIRTUAL_START + (i * PAGE_SIZE) as u64) },
            gsi_base,
        ));
    }

    setup_ioapic_timer(&mut ioapics, &final_lapic);
}

#[allow(static_mut_refs)]
fn setup_ioapic_timer(ioapics: &mut [(x2apic::ioapic::IoApic, u32)], lapic: &x2apic::lapic::LocalApic) {
    for (ioapic, gsi_base) in ioapics.iter_mut() {
        if *gsi_base != 0 {
            continue;
        }
        let mut entry = RedirectionTableEntry::default();
        entry.set_vector(IOAPIC_TIMER_VECTOR);
        entry.set_dest(unsafe { lapic.id() } as u8);
        entry.set_mode(IrqMode::Fixed);
        entry.set_flags(IrqFlags::MASKED); // mask it

        unsafe { ioapic.set_table_entry(IOAPIC_TIMER_INPUT, entry) };
    }

    unsafe {
        (*IDT.as_mut_ptr())[LAPIC_TIMER_VECTOR]
            .set_handler_fn(lapic_timer_handler)
    };

    for (ioapic, gsi_base) in ioapics.iter_mut() {
        if *gsi_base != 0 {
            continue;
        }
        unsafe { ioapic.enable_irq(IOAPIC_TIMER_INPUT) };
    }
}

extern "x86-interrupt" fn lapic_timer_handler(_stack_frame: InterruptStackFrame) {
    unsafe { Msr::new(X2APIC_EOI_MSR).write(0) };
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

/// Get IO APIC physical addresses using ACPI.
/// returns a tuple of (address, global_system_interrupt_base)
fn get_ioapic_info(rsdp_addr: usize, memory_offset: usize) -> Vec<(u32, u32)> {
    let tables = unsafe { AcpiTables::from_rsdp(KernelAcpiHandler::new(memory_offset), rsdp_addr).unwrap() };
    let platform_info = tables.platform_info().unwrap();

    let mut ioapic_addrs = Vec::new();
    if let InterruptModel::Apic(apic) = &platform_info.interrupt_model {
        for ioapic in apic.io_apics.iter() {
            ioapic_addrs.push((ioapic.address, ioapic.global_system_interrupt_base));
        }
    }
    ioapic_addrs
}

fn map_lapic_registers(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    lapic_mmio: PhysAddr,
    virtaddr: VirtAddr,
) {
    unsafe {
        mapper.map_to(
            Page::containing_address(virtaddr),
            PhysFrame::containing_address(lapic_mmio),
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE | PageTableFlags::NO_EXECUTE,
            frame_allocator,
        )
        .expect("failed to map lapic")
        .flush();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApicSupport {
    X2Apic,
    XApic,
    None,
}

fn detect_lapic_support() -> ApicSupport {
    let mut ecx: u32;
    let mut edx: u32;
    unsafe {
        core::arch::asm!(
            "cpuid",
            lateout("ecx") ecx,
            lateout("edx") edx,
        );
    }
    if (ecx & (1 << 21)) != 0 {
        ApicSupport::X2Apic
    } else if (edx & (1 << 9)) != 0 {
        ApicSupport::XApic
    } else {
        ApicSupport::None
    }
}
