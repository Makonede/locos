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
#[cfg(feature = "log-error")]
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::serial_println!("\x1B[31mERROR:\x1B[0m {}", format_args!($($arg)*));
    };
}

/// No-op error macro when log-error feature is disabled.
#[cfg(not(feature = "log-error"))]
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {};
}

/// Logs a warning message with a yellow "WARN: " prefix.
#[cfg(feature = "log-warn")]
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::serial_println!("\x1B[33mWARN:\x1B[0m {}", format_args!($($arg)*));
    };
}

/// No-op warn macro when log-warn feature is disabled.
#[cfg(not(feature = "log-warn"))]
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {};
}

/// Logs an info message with a green "INFO: " prefix.
#[cfg(feature = "log-info")]
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::serial_println!("\x1B[32mINFO:\x1B[0m {}", format_args!($($arg)*));
    };
}

/// No-op info macro when log-info feature is disabled.
#[cfg(not(feature = "log-info"))]
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {};
}

/// Logs a debug message with a green "DEBUG: " prefix.
#[cfg(feature = "log-debug")]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::serial_println!("\x1B[32mDEBUG:\x1B[0m {}", format_args!($($arg)*));
    };
}

/// No-op debug macro when log-debug feature is disabled.
#[cfg(not(feature = "log-debug"))]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

/// Logs a trace message with a light blue "TRACE: " prefix.
#[cfg(feature = "log-trace")]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::serial_println!("\x1B[36mTRACE:\x1B[0m {}", format_args!($($arg)*));
    };
}

/// No-op trace macro when log-trace feature is disabled.
#[cfg(not(feature = "log-trace"))]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {};
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
