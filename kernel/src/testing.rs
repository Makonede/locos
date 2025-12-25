//! Test framework for locOS.
//!
//! Provides test runner and utilities for kernel tests.

use crate::{serial_print, serial_println};

/// QEMU exit codes for test results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

/// Exit QEMU with the given exit code
pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

/// Trait for items that can be tested
pub trait Testable {
    /// Run the test
    fn run(&self) -> ();
    /// Get the test name
    fn name(&self) -> &'static str;
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        let test_name = core::any::type_name::<T>();
        serial_print!("{}...\t", test_name);
        self();
        if self.name().contains("multitasking") {
            serial_println!("[scheduled]");
            return;
        }
        serial_println!("[ok]");
    }

    fn name(&self) -> &'static str {
        core::any::type_name::<T>()
    }
}

/// Test runner that executes all tests
#[cfg(test)]
pub fn test_runner(tests: &[&dyn Testable]) {
    use crate::{hcf, serial_print, serial_println, tasks::scheduler::kinit_multitasking};

    //serial_print!("\x1b[2J\x1b[H");
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }

    kinit_multitasking();

    x86_64::instructions::interrupts::enable();
    unsafe {
        use crate::interrupts::apic::LAPIC_TIMER_VECTOR;
        core::arch::asm!("int {}", const LAPIC_TIMER_VECTOR);
    }

    hcf();
}
