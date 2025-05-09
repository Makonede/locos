use crate::info;
use conquer_once::spin::Lazy;
use x86_64::{
    VirtAddr,
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
};

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
pub const TIMER_IST_INDEX: u16 = 1;

/// The Global Descriptor Table and its selectors.
static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let kernel_code_selector = gdt.append(Descriptor::kernel_code_segment());
    gdt.append(Descriptor::kernel_data_segment());
    let user_code_selector = gdt.append(Descriptor::user_code_segment());
    gdt.append(Descriptor::user_data_segment());
    let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));
    (
        gdt,
        Selectors {
            kernel_code_selector,
            user_code_selector,
            tss_selector,
        },
    )
});

/// merged struct for storing selectors to user code, kernel code, and the TSS.
#[allow(dead_code)] // remove in future
struct Selectors {
    kernel_code_selector: SegmentSelector,
    user_code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

/// Initialize the Global Descriptor Table.
/// Must be called before using any other GDT functions, such as setting up the TSS.
pub fn init_gdt() {
    use x86_64::instructions::segmentation::Segment;

    GDT.0.load();
    unsafe {
        x86_64::instructions::segmentation::CS::set_reg(GDT.1.kernel_code_selector);
        x86_64::instructions::tables::load_tss(GDT.1.tss_selector);
    }

    info!("gdt initialized");
}

/// Set up the Task State Segment (TSS) with an interrupt stack.
static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 4096 * 5;
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
        let stack_start = VirtAddr::from_ptr(&raw const STACK);
        stack_start + STACK_SIZE as u64
    };

    tss.interrupt_stack_table[TIMER_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 4096 * 5;
        static mut TIMER_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
        let stack_start = VirtAddr::from_ptr(&raw const TIMER_STACK);
        stack_start + STACK_SIZE as u64
    };

    info!("tss initialized");
    tss
});
