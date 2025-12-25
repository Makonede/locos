//! Global Descriptor Table (GDT) setup and management.
//!
//! Provides GDT initialization with kernel and user mode segments,
//! and Task State Segment (TSS) configuration.

use crate::info;
use conquer_once::spin::Lazy;
use x86_64::{
    VirtAddr,
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
};

/// Index for the double fault interrupt stack in the TSS
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// Kernel code segment index in the GDT
pub const KERNEL_CODE_SEGMENT_INDEX: u16 = 1;
/// Kernel data segment index in the GDT
pub const KERNEL_DATA_SEGMENT_INDEX: u16 = 2;
/// User code segment index in the GDT
pub const USER_CODE_SEGMENT_INDEX: u16 = 3;
/// User data segment index in the GDT
pub const USER_DATA_SEGMENT_INDEX: u16 = 4;
/// TSS segment index in the GDT
pub const TSS_SEGMENT_INDEX: u16 = 5;

/// The Global Descriptor Table and its selectors.
static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let kernel_code_selector = gdt.append(Descriptor::kernel_code_segment());
    let kernel_data_selector = gdt.append(Descriptor::kernel_data_segment());
    let user_code_selector = gdt.append(Descriptor::user_code_segment());
    let user_data_selector = gdt.append(Descriptor::user_data_segment());
    let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));
    (
        gdt,
        Selectors {
            kernel_code_selector,
            kernel_data_selector,
            user_code_selector,
            user_data_selector,
            tss_selector,
        },
    )
});

/// Selectors for kernel and user mode segments
struct Selectors {
    kernel_code_selector: SegmentSelector,
    kernel_data_selector: SegmentSelector,
    user_code_selector: SegmentSelector,
    user_data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

/// Initialize the Global Descriptor Table
///
/// Must be called before using any other GDT functions, such as setting up the TSS.
pub fn init_gdt() {
    use x86_64::instructions::segmentation::Segment;

    GDT.0.load();
    unsafe {
        use x86_64::instructions::segmentation::{CS, DS, ES, SS};
        // Set up code and data segments
        CS::set_reg(GDT.1.kernel_code_selector);
        DS::set_reg(GDT.1.kernel_data_selector);
        ES::set_reg(GDT.1.kernel_data_selector);
        SS::set_reg(GDT.1.kernel_data_selector);
        // Load TSS
        x86_64::instructions::tables::load_tss(GDT.1.tss_selector);
    }

    info!("gdt initialized");
}

/// Task State Segment with interrupt stack
static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 4096 * 5;
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
        let stack_start = VirtAddr::from_ptr(&raw const STACK);
        stack_start + STACK_SIZE as u64
    };

    info!("tss initialized");
    tss
});

/// Update the TSS RSP0 field with the kernel stack for the current task
///
/// This is used by the CPU when transitioning from user mode to kernel mode via interrupts.
///
/// # Safety
/// Must be called with a valid kernel stack pointer.
pub unsafe fn set_kernel_stack(stack_top: VirtAddr) {
    let tss_ptr = &raw const *TSS as *mut TaskStateSegment;
    unsafe {
        (*tss_ptr).privilege_stack_table[0] = stack_top;
    }
}
