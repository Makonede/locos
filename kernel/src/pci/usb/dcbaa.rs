use core::ptr::write_bytes;

use crate::{memory::FRAME_ALLOCATOR, pci::usb::xhci_registers::XhciRegisters, debug};

/// Initialize the Device Context Base Address Array (DCBAA)
/// 
/// Should pass in a xchi registers ref
pub fn init_dcbaa(xhci_regs: &mut XhciRegisters) {
    let needed_entries = xhci_regs.capability().hcs_params1.max_device_slots() + 1;

    let dcbaa_size = needed_entries as usize * core::mem::size_of::<u64>();
    let frames_needed = dcbaa_size.div_ceil(4096).next_power_of_two();

    let mut lock = FRAME_ALLOCATOR.lock();
    let allocator = lock.as_mut().unwrap();
    let dcbaa_virt = allocator.allocate_pages(frames_needed)
        .expect("Failed to allocate frames for DCBAA");

    // zero out pages
    unsafe {
        write_bytes(dcbaa_virt.as_mut_ptr::<()>(), 0, frames_needed * 4096);
    }

    let dcbaa_phys = dcbaa_virt.as_u64() - allocator.hddm_offset;
    xhci_regs.set_device_context_base_addr(dcbaa_phys);
    debug!("Allocated DCBAA at {:#x} with {} entries", dcbaa_phys, needed_entries);
}

