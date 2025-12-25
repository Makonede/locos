//! USB (Universal Serial Bus) driver for locOS.
//!
//! Provides xHCI controller support for USB 3.0 devices.

pub mod xhci;
pub mod init_helpers;
pub mod xhci_registers;

/// Initialize USB subsystem (see xhci module)
pub fn init() {
    xhci::xhci_init();
}
