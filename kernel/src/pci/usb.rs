pub mod xhci;
pub mod xhci_registers;

pub fn init() {
    xhci::xhci_init();
}
