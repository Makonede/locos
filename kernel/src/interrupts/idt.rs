use crate::{info, tasks::scheduler::try_grow_user_stack};
use spin::Lazy;
use x86_64::{registers::control::Cr2, structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode}};

use crate::{println, serial_println};

/// Interrupt Descriptor Table with handlers for interrupts.
/// Current supported interrupts:
/// - Breakpoint
/// - Page Fault
/// - Double Fault
/// - General Protection Fault
pub static mut IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.general_protection_fault
        .set_handler_fn(general_proction_fault_handler);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
    }
    info!("idt initialized");
    idt
});

/// Initialize the Interrupt Descriptor Table.
pub fn init_idt() {
    unsafe { (*IDT).load() };
    info!("idt loaded");
}

/// Breakpoint exception handler
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

/// Page fault exception handler
///
/// Attempts to grow the user stack if the fault occurred in user mode
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    let fault_addr = Cr2::read().expect("Failed to read CR2");

    if error_code.contains(PageFaultErrorCode::USER_MODE)
        && unsafe { try_grow_user_stack(fault_addr).is_ok() } {
            return;
        }

    panic!(
        "EXCEPTION: PAGE FAULT at {:#x}\n{:#?}\nWith error: {:#?}",
        fault_addr, stack_frame, error_code,
    );
}

/// General protection fault handler
extern "x86-interrupt" fn general_proction_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: GENERAL PROTECTION FAULT\n{:#?}\nWith error: {:#?}",
        stack_frame, error_code
    )
}

/// Double fault exception handler
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}
