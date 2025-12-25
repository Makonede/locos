//! Advanced Programmable Interrupt Controller (APIC) support.
//!
//! Provides APIC initialization and interrupt handling using x2APIC.

use crate::{error, info, pci::nvme::{NVME_ADMIN_VECTOR, NVME_IO_VECTOR}, tasks::scheduler::schedule, warn};
use acpi::{
    AcpiHandler, AcpiTables, InterruptModel,
    handler::PhysicalMapping,
    madt::{InterruptSourceOverrideEntry, Madt, MadtEntry},
};
use alloc::vec::Vec;
use core::ptr::NonNull;
use x2apic::{
    ioapic::{IrqFlags, IrqMode, RedirectionTableEntry},
    lapic::{LocalApicBuilder, xapic_base},
};
use x86_64::{
    PhysAddr, VirtAddr,
    instructions::port::Port,
    registers::model_specific::Msr,
    structures::{
        idt::InterruptStackFrame,
        paging::{Mapper, Page, PageTableFlags, PhysFrame, Size4KiB},
    },
};

use crate::{
    debug,
    memory::{FRAME_ALLOCATOR, PAGE_TABLE},
};

use super::{idt::IDT, pic::disable_legacy_pics};

const PAGE_SIZE: usize = 0x1000;
const X2APIC_EOI_MSR: u32 = 0x80B;

const IOAPICS_VIRTUAL_START: u64 = 0xFFFF_F000_0000_0000;
const XAPIC_VIRTUAL_START: u64 = 0xFFFF_F100_0000_0000;
const ACPI_MAPPINGS_START: u64 = 0xFFFF_F200_0000_0000;
pub const LAPIC_TIMER_VECTOR: u8 = 0x30;
const LAPIC_ERROR_VECTOR: u8 = 0x31;
const LAPIC_SPURIOUS_VECTOR: u8 = 0xFF;
const IOAPIC_TIMER_VECTOR: u8 = 0x20;
const IOAPIC_TIMER_INPUT: u8 = 0;
const KEYBOARD_VECTOR: u8 = 0x21;
const KEYBOARD_IRQ: u8 = 1;
const TIMER_RELOAD: u16 = (1193182u32 / 20) as u16;

/// Interrupt handler for the PIT.
///
/// Acknowledges the interrupt by writing to the EOI MSR.
extern "x86-interrupt" fn ioapic_timer_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        Msr::new(X2APIC_EOI_MSR).write(0);
    };
}

extern "x86-interrupt" fn spurious_handler(_stack_frame: InterruptStackFrame) {
    warn!("spurious interrupt received");

    unsafe {
        Msr::new(X2APIC_EOI_MSR).write(0);
    };
}

extern "x86-interrupt" fn lapic_error_handler(_stack_frame: InterruptStackFrame) {
    warn!("error interrupt received");

    unsafe {
        Msr::new(X2APIC_EOI_MSR).write(0);
    };
}

extern "x86-interrupt" fn keyboard_handler(_stack_frame: InterruptStackFrame) {
    crate::ps2::keyboard::handle_interrupt();

    unsafe {
        Msr::new(X2APIC_EOI_MSR).write(0);
    };
}

extern "x86-interrupt" fn nvme_admin_handler(_stack_frame: InterruptStackFrame) {
    crate::pci::nvme::handle_admin_interrupt();

    unsafe {
        Msr::new(X2APIC_EOI_MSR).write(0);
    };
}

extern "x86-interrupt" fn nvme_io_handler(_stack_frame: InterruptStackFrame) {
    crate::pci::nvme::handle_io_interrupt();

    unsafe {
        Msr::new(X2APIC_EOI_MSR).write(0);
    };
}

/// Sets up the Local APIC and enables it using the x2apic crate.
///
/// # Safety
/// Must be called after IDT is loaded
#[allow(static_mut_refs)]
pub unsafe fn setup_apic(rsdp_addr: usize) {
    disable_legacy_pics();

    let mut builder = LocalApicBuilder::new();
    let mut lapic = builder
        .timer_vector(LAPIC_TIMER_VECTOR as usize)
        .error_vector(LAPIC_ERROR_VECTOR as usize)
        .spurious_vector(LAPIC_SPURIOUS_VECTOR as usize);

    match detect_lapic_support() {
        ApicSupport::XApic => {
            let lapic_base = unsafe { xapic_base() };
            map_lapic_registers(
                PhysAddr::new(lapic_base),
                VirtAddr::new(XAPIC_VIRTUAL_START),
            );
            lapic = lapic.set_xapic_base(XAPIC_VIRTUAL_START);
            error!(
                "no x2apic support detected, using xAPIC. this will cause issues with the global timer"
            );
        }
        ApicSupport::None => {
            panic!("No APIC support detected");
        }
        ApicSupport::X2Apic => (),
    }

    let mut final_lapic = lapic.build().unwrap();

    unsafe {
        (&mut (*IDT.as_mut_ptr()))[LAPIC_TIMER_VECTOR]
            .set_handler_addr(VirtAddr::new(schedule as usize as u64));
        (&mut (*IDT.as_mut_ptr()))[LAPIC_ERROR_VECTOR].set_handler_fn(lapic_error_handler);
        (&mut (*IDT.as_mut_ptr()))[LAPIC_SPURIOUS_VECTOR].set_handler_fn(spurious_handler);
        (&mut (*IDT.as_mut_ptr()))[KEYBOARD_VECTOR].set_handler_fn(keyboard_handler);
        (&mut (*IDT.as_mut_ptr()))[NVME_ADMIN_VECTOR].set_handler_fn(nvme_admin_handler);
        (&mut (*IDT.as_mut_ptr()))[NVME_IO_VECTOR].set_handler_fn(nvme_io_handler);
    }

    unsafe { final_lapic.enable() };

    // IO apic
    let mut tables = unsafe { AcpiTables::from_rsdp(KernelAcpiHandler, rsdp_addr).unwrap() };
    let ioapic_addrs = get_ioapic_info(&mut tables);
    if ioapic_addrs.is_empty() {
        panic!("No IO APIC found");
    }

    for (virtaddr, &(ioapic_mmio, _)) in (IOAPICS_VIRTUAL_START..)
        .step_by(PAGE_SIZE)
        .zip(ioapic_addrs.iter())
    {
        let virtaddr = VirtAddr::new(virtaddr);
        let ioapic_mmio = PhysAddr::new(ioapic_mmio as u64);

        // Map the IO APIC MMIO region to the virtual address space
        unsafe { map_ioapic(ioapic_mmio, virtaddr) };
    }

    let mut ioapics = Vec::with_capacity(ioapic_addrs.len());
    for (i, &(_, gsi_base)) in ioapic_addrs.iter().enumerate() {
        ioapics.push((
            unsafe { x2apic::ioapic::IoApic::new(IOAPICS_VIRTUAL_START + (i * PAGE_SIZE) as u64) },
            gsi_base,
        ));
    }

    let mut interrupt_source_overrides = get_interrupt_source_overrides(&mut tables);
    let timer_override = interrupt_source_overrides
        .iter_mut()
        .find(|x| x.irq == IOAPIC_TIMER_INPUT);

    debug!("Timer override: {:?}", timer_override);

    let timer_gsi = if let Some(timer_override) = timer_override {
        timer_override.global_system_interrupt
    } else {
        IOAPIC_TIMER_INPUT as u32
    };

    unsafe {
        setup_pit_timer(TIMER_RELOAD);
    }
    setup_ioapic_timer(&mut ioapics, timer_gsi, unsafe { final_lapic.id() } as u8);

    let keyboard_override = interrupt_source_overrides
        .iter()
        .find(|x| x.irq == KEYBOARD_IRQ);


    let keyboard_gsi = if let Some(keyboard_override) = keyboard_override {
        keyboard_override.global_system_interrupt
    } else {
        KEYBOARD_IRQ as u32
    };

    setup_ioapic_keyboard(&mut ioapics, keyboard_gsi, unsafe { final_lapic.id() } as u8);

    info!("apic initialized with {} IO APICs", ioapic_addrs.len());
}

#[allow(static_mut_refs)]
/// Configures the IOAPIC timer and sets up the LAPIC timer interrupt handler.
///
/// This function masks the IOAPIC timer, assigns the interrupt vector,
/// and enables the IRQ for the timer input. It also installs the LAPIC timer handler
/// in the IDT and enabled the PIT.
fn setup_ioapic_timer(ioapics: &mut [(x2apic::ioapic::IoApic, u32)], timer_gsi: u32, lapic_id: u8) {
    for (ioapic, gsi_base) in ioapics.iter_mut() {
        if !(*gsi_base..*gsi_base + unsafe { ioapic.max_table_entry() } as u32 + 1)
            .contains(&timer_gsi)
        {
            continue;
        }
        let mut entry = RedirectionTableEntry::default();
        entry.set_vector(IOAPIC_TIMER_VECTOR);
        entry.set_dest(lapic_id);
        entry.set_mode(IrqMode::Fixed);
        entry.set_flags(IrqFlags::MASKED); // mask it

        unsafe { ioapic.set_table_entry((timer_gsi - *gsi_base) as u8, entry) };
    }

    unsafe { (&mut (*IDT.as_mut_ptr()))[IOAPIC_TIMER_VECTOR].set_handler_fn(ioapic_timer_handler) };

    for (ioapic, gsi_base) in ioapics.iter_mut() {
        if !(*gsi_base..*gsi_base + unsafe { ioapic.max_table_entry() } as u32 + 1)
            .contains(&timer_gsi)
        {
            continue;
        }
        unsafe { ioapic.enable_irq((timer_gsi - *gsi_base) as u8) };
    }

    debug!("IOAPIC timer setup");
}

/// Configure the IOAPIC keyboard interrupt
fn setup_ioapic_keyboard(ioapics: &mut [(x2apic::ioapic::IoApic, u32)], keyboard_gsi: u32, lapic_id: u8) {
    info!("Setting up IOAPIC keyboard interrupt: GSI={}, LAPIC_ID={}", keyboard_gsi, lapic_id);

    for (ioapic, gsi_base) in ioapics.iter_mut() {
        if !(*gsi_base..*gsi_base + unsafe { ioapic.max_table_entry() } as u32 + 1)
            .contains(&keyboard_gsi)
        {
            continue;
        }

        info!("Configuring keyboard interrupt on IOAPIC with GSI base {}", gsi_base);

        let mut entry = RedirectionTableEntry::default();
        entry.set_vector(KEYBOARD_VECTOR);
        entry.set_dest(lapic_id);
        entry.set_mode(IrqMode::Fixed);
        entry.set_flags(IrqFlags::MASKED);

        unsafe { ioapic.set_table_entry((keyboard_gsi - *gsi_base) as u8, entry) };

        info!("Keyboard interrupt entry configured, now enabling...");
    }

    for (ioapic, gsi_base) in ioapics.iter_mut() {
        if !(*gsi_base..*gsi_base + unsafe { ioapic.max_table_entry() } as u32 + 1)
            .contains(&keyboard_gsi)
        {
            continue;
        }
        unsafe { ioapic.enable_irq((keyboard_gsi - *gsi_base) as u8) };
        info!("Keyboard interrupt enabled on IOAPIC");
    }

    info!("IOAPIC keyboard interrupt setup complete");
}

/// Set up the PIT (Programmable Interval Timer) channel 0 in mode 2 (rate generator).
unsafe fn setup_pit_timer(reload: u16) {
    let mut pit_mode_port = Port::<u8>::new(0x43);
    let mut pit_data_port = Port::<u8>::new(0x40);

    unsafe {
        pit_mode_port.write(0b00110100); // channel 0, mode 2 (rate generator), binary
        pit_data_port.write((reload & 0xFF) as u8); // Low byte
        pit_data_port.write((reload >> 8) as u8); // High byte
    }
}

fn get_interrupt_source_overrides(
    tables: &mut AcpiTables<KernelAcpiHandler>,
) -> Vec<InterruptSourceOverrideEntry> {
    let pin = tables.find_table::<Madt>().unwrap();
    let madt = pin.get();
    madt.entries()
        .filter_map(|x| {
            if let MadtEntry::InterruptSourceOverride(iso) = x {
                Some(*iso)
            } else {
                None
            }
        })
        .collect()
}

/// Maps the IO apic memory adress to the virtual address space.
///
/// # Safety
/// Fundamentally unsafe due to mapping pages
/// Maps the IO apic memory adress to the virtual address space.
unsafe fn map_ioapic(ioapic_mmio: PhysAddr, virtaddr: VirtAddr) {
    unsafe {
        PAGE_TABLE
            .lock()
            .as_mut()
            .unwrap()
            .map_to(
                Page::<Size4KiB>::containing_address(virtaddr),
                PhysFrame::containing_address(ioapic_mmio),
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::NO_CACHE
                    | PageTableFlags::NO_EXECUTE,
                FRAME_ALLOCATOR.lock().as_mut().unwrap(),
            )
            .expect("failed to map io apic")
            .flush();
    }
}
/// Minimal handler for ACPI physical memory mapping.
#[derive(Clone, Copy)]
pub struct KernelAcpiHandler;

impl AcpiHandler for KernelAcpiHandler {
    /// Maps a physical memory region for ACPI use.
    /// # Safety
    /// This function is unsafe due to raw pointer and static mut usage.
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        // Use static mut for next available virtual address (single-threaded assumption).
        static mut NEXT_ACPI_VIRT: u64 = ACPI_MAPPINGS_START;

        let phys_addr = physical_address as u64;
        let offset = (phys_addr & (PAGE_SIZE as u64 - 1)) as usize;
        let total_size = offset + size;
        let num_pages = total_size.div_ceil(PAGE_SIZE);

        // Allocate a contiguous virtual region for the mapping.
        let virt_base = {
            let addr = unsafe { NEXT_ACPI_VIRT };
            unsafe { NEXT_ACPI_VIRT += (num_pages * PAGE_SIZE) as u64 };
            addr
        };

        // Lock and get page table and frame allocator.
        let mut page_table_guard = PAGE_TABLE.lock();
        let page_table = page_table_guard
            .as_mut()
            .expect("PAGE_TABLE not initialized");
        let mut frame_allocator_guard = FRAME_ALLOCATOR.lock();
        let frame_allocator = frame_allocator_guard
            .as_mut()
            .expect("FRAME_ALLOCATOR not initialized");

        let flags = PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::NO_CACHE
            | PageTableFlags::NO_EXECUTE;

        // Map each page in the region.
        for i in 0..num_pages {
            let virt = VirtAddr::new(virt_base + (i as u64) * PAGE_SIZE as u64);
            let phys = PhysAddr::new(
                (phys_addr & !(PAGE_SIZE as u64 - 1)) + (i as u64) * PAGE_SIZE as u64,
            );
            let page = Page::<Size4KiB>::containing_address(virt);
            let frame = PhysFrame::containing_address(phys);
            // Safety: mapping physical memory, must ensure no overlap.
            unsafe {
                page_table
                    .map_to(page, frame, flags, frame_allocator)
                    .expect("failed to map ACPI region")
                    .flush()
            };
        }

        let virt_addr = virt_base + offset as u64;

        unsafe {
            PhysicalMapping::new(
                physical_address,
                NonNull::new(virt_addr as *mut T).expect("Null virtual address in ACPI mapping"),
                size,
                num_pages * PAGE_SIZE,
                *self,
            )
        }
    }
    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {}
}

/// Get IO APIC physical addresses using ACPI.
/// returns a tuple of (address, global_system_interrupt_base)
/// Retrieves IO APIC physical addresses using ACPI tables.
///
/// # Arguments
/// * `rsdp_addr` - The physical address of the ACPI RSDP structure.
///
/// # Returns
/// A vector of tuples containing (IOAPIC address, global system interrupt base).
fn get_ioapic_info(tables: &mut AcpiTables<KernelAcpiHandler>) -> Vec<(u32, u32)> {
    let platform_info = tables.platform_info().unwrap();

    let mut ioapic_addrs = Vec::new();
    if let InterruptModel::Apic(apic) = &platform_info.interrupt_model {
        for ioapic in apic.io_apics.iter() {
            ioapic_addrs.push((ioapic.address, ioapic.global_system_interrupt_base));
        }
    }
    ioapic_addrs
}

/// Maps the LAPIC registers to the virtual address space.
///
/// # Safety
/// This function is unsafe because it manipulates page tables and maps physical memory.
fn map_lapic_registers(lapic_mmio: PhysAddr, virtaddr: VirtAddr) {
    unsafe {
        PAGE_TABLE
            .lock()
            .as_mut()
            .unwrap()
            .map_to(
                Page::<Size4KiB>::containing_address(virtaddr),
                PhysFrame::containing_address(lapic_mmio),
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::NO_CACHE
                    | PageTableFlags::NO_EXECUTE,
                FRAME_ALLOCATOR.lock().as_mut().unwrap(),
            )
            .expect("failed to map lapic")
            .flush();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Represents the supported APIC modes on this system.
enum ApicSupport {
    /// x2APIC mode is supported.
    X2Apic,
    /// xAPIC mode is supported.
    XApic,
    /// No APIC support detected.
    None,
}

/// Detects the available Local APIC support on the current processor.
///
/// Returns the type of APIC supported (x2APIC, xAPIC, or none).
fn detect_lapic_support() -> ApicSupport {
    let mut ecx: u32;
    let mut edx: u32;
    unsafe {
        core::arch::asm!(
            "cpuid",
            in("eax") 1,
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
