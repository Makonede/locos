use x86_64::{structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, PhysFrame, Size4KiB}, PhysAddr, VirtAddr};

use super::pic::disable_legacy_pics;

const IA32_APIC_BASE_MSR: u32 = 0x1B;
const LAPIC_VIRTUAL_START: u64 = 0xFFFF_FFFE_C000_0000;
const LAPIC_SVR_OFFSET: u32 = 0xF0;
const SVR_VALUE: u32 = LAPIC_ENABLE | 0xFF;
const LAPIC_ENABLE: u32 = 1 << 8;
const LAPIC_TPR_OFFSET: u32 = 0x80;

/// Sets up the Local APIC and enables it. Maps lapic register frame to virtual memory.
/// 
/// # Safety
/// Must be called after IDT is loaded
pub unsafe fn setup_apic(mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) {
    disable_legacy_pics();
    enable_apic();

    let apic_location = get_base_addr();
    unsafe { map_lapic_registers(mapper, frame_allocator, apic_location) };

    unsafe {
        write_lapic_register(SVR_VALUE, LAPIC_SVR_OFFSET);
        write_lapic_register(0, LAPIC_TPR_OFFSET);
    };
}

/// Writes a value to the Local APIC register at the specified offset.
/// 
/// # Safety
/// The lapic register frame MUST be mapped to virtual memory
pub unsafe fn write_lapic_register(value: u32, offset: u32) {
    let virtual_addr = (LAPIC_VIRTUAL_START + offset as u64) as *mut u32;
    unsafe {
        virtual_addr.write_volatile(value);
    }
}

/// Maps the Local APIC register frame to virtual memory.
/// 
/// # Safety
/// Fundamentally unsafe due to mapping pages
unsafe fn map_lapic_registers(mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>, apic_location: PhysAddr) {
    unsafe {
        mapper.map_to(
            Page::containing_address(VirtAddr::new(LAPIC_VIRTUAL_START)),
            PhysFrame::containing_address(apic_location),
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE | PageTableFlags::NO_EXECUTE,
            frame_allocator,
        ).expect("failed to map LAPIC registers").flush();
    }
}

fn enable_apic() {
    unsafe {
        let mut msr = x86_64::registers::model_specific::Msr::new(0x1B);
        let base = msr.read();

        let new_base = base | 0x800;
        msr.write(new_base);
    }
}

fn get_base_addr() -> PhysAddr {
    unsafe {
        let msr = x86_64::registers::model_specific::Msr::new(IA32_APIC_BASE_MSR);
        let base = msr.read();

        let addr = base & 0xFFFF_FFFF_FFFF_F000;
        PhysAddr::new(addr)
    }
}
