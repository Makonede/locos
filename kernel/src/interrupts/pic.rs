use x86_64::instructions::port::Port;

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const PIC1_OFFSET: u8 = 0x20;
const PIC2_OFFSET: u8 = 0x28;

const ALL_INTERRUPTS_MASK: u8 = 0xFF;

pub fn disable_legacy_pics() {
    init_and_remap_pics();
    mask_all_irqs();
}

fn init_and_remap_pics() {
    unsafe {
        let mut master_port = Port::new(PIC1_COMMAND);
        master_port.write(0x11u8);
        let mut master_data_port = Port::new(PIC1_DATA);
        master_data_port.write(PIC1_OFFSET); // Remap offset to 32
        master_data_port.write(0x04); // Tell PIC1 that there is slave PIC
        master_data_port.write(0x01);

        let mut slave_port = Port::new(PIC2_COMMAND);
        slave_port.write(0x11u8);
        let mut slave_data_port = Port::new(PIC2_DATA);
        slave_data_port.write(PIC2_OFFSET); // Remap offset to 40
        slave_data_port.write(0x02); // Tell PIC2 its cascade identity
        slave_data_port.write(0x01);
    }
}

fn mask_all_irqs() {
    unsafe {
        let mut master_port = Port::new(PIC1_DATA);
        master_port.write(ALL_INTERRUPTS_MASK);
        let mut slave_port = Port::new(PIC2_DATA);
        slave_port.write(ALL_INTERRUPTS_MASK);
    }
}
