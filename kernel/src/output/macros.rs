//! Macros for printing to the framebuffer using the global terminal instance.

/// Global print! macro that writes to the framebuffer.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
            use $crate::output::FLANTERM;
            let mut lock = FLANTERM.lock();
            if let Some(writer) = lock.as_mut() {
                write!(writer, $($arg)*).unwrap();
            }
        }
    };
}

/// Logs an error message with a red "ERROR: " prefix.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::println!("\x1b[31mERROR:\x1b[0m {}", format_args!($($arg)*));
        $crate::serial_println!("\x1b[31mERROR:\x1b[0m {}", format_args!($($arg)*));
    };
}

/// Logs a warning message with a yellow "WARN: " prefix.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::println!("\x1b[33mWARN:\x1b[0m {}", format_args!($($arg)*));
        $crate::serial_println!("\x1b[33mWARN:\x1b[0m {}", format_args!($($arg)*));
    };
}

/// Logs an info message with a green "INFO: " prefix.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::println!("\x1b[32mINFO:\x1b[0m {}", format_args!($($arg)*));
        $crate::serial_println!("\x1b[32mINFO:\x1b[0m {}", format_args!($($arg)*));
    };
}

/// Logs a debug message with a green "DEBUG: " prefix.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::println!("\x1b[32mDEBUG:\x1b[0m {}", format_args!($($arg)*));
        $crate::serial_println!("\x1b[32mDEBUG:\x1b[0m {}", format_args!($($arg)*));
    };
}

/// Logs a trace message with a light blue "TRACE: " prefix.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::println!("\x1b[36mTRACE:\x1b[0m {}", format_args!($($arg)*));
        $crate::serial_println!("\x1b[36mTRACE:\x1b[0m {}", format_args!($($arg)*));
    };
}

/// Global println! macro that writes to the framebuffer.
#[macro_export]
macro_rules! println {
    () => {
        {
            $crate::print!("\n");
        }
    };
    ($($arg:tt)*) => {
        {
            $crate::print!("{}\n", format_args!($($arg)*));
        }
    };
}
