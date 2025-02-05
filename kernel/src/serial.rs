use uart_16550::SerialPort;
use conquer_once::spin::Lazy;
use spin::Mutex;


pub static SERIAL1: Lazy<Mutex<SerialPort>> = Lazy::new(|| {
    let mut serial_port = unsafe { SerialPort::new(0x3F8) };
    serial_port.init();
    Mutex::new(serial_port)
});

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        use core::fmt::Write;
        use crate::serial::SERIAL1;
        let _ = write!(SERIAL1.lock(), $($arg)*);
    };
}

#[macro_export]
macro_rules! serial_println {
    () => {
        serial_print!("\n");
    };
    ($($arg:tt)*) => {
        serial_print!("{}\n", format_args!($($arg)*));
    };
}
