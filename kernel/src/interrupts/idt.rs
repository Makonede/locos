use spin::Lazy;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::{println, serial_println};

/// Interrupt Descriptor Table with handlers for inturrupts.
/// Current supported interrupts:
/// - Breakpoint
/// - Page Fault
/// - Double Fault
pub static mut IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
    }
    idt
});

/// Initialize the Interrupt Descriptor Table.
pub fn init_idt() {
    unsafe { (*IDT).load() };
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    panic!(
        "EXCEPTION: PAGE FAULT\n{:#?}\nWith error: {:#?}",
        stack_frame, error_code,
    );
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}
