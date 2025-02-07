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
    ($($arg:tt)*) => {{
        // Use absolute paths to prevent conflicts
        let _ = ::core::fmt::Write::write_fmt(
            &mut *$crate::serial::SERIAL1.lock(),
            format_args!($($arg)*)
        );
    }};
}

#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::serial_print!("\n");
    };
    ($($arg:tt)*) => {
        $crate::serial_print!("{}\n", format_args!($($arg)*));
    };
}
