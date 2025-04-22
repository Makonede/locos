//! Provides a terminal emulator implementation using the flanterm library.
//!
//! This module implements a terminal emulator that provides:
//! - Full terminal emulation capabilities via the flanterm library
//! - Direct framebuffer writing
//! - ANSI escape sequence support
//! - Safe Rust interface around the unsafe flanterm C library
//! 
//! The main components are:
//! - `FlanConsole`: The main terminal emulator struct that implements `Write`
//! - `FLANTERM`: A global static instance accessible throughout the kernel
//! - `flanterm_init`: Initialization function to set up the terminal

use core::{fmt::Write, ptr};

use flanterm::sys::{flanterm_context, flanterm_fb_init, flanterm_write};
use spin::Mutex;

use super::framebuffer::FramebufferInfo;

/// Global terminal instance protected by a mutex.
/// 
/// This static is initialized by `flanterm_init` and can be accessed
/// throughout the kernel for terminal operations.
pub static FLANTERM: Mutex<Option<FlanConsole>> = Mutex::new(None);

/// Initializes the global terminal instance.
///
/// # Arguments
///
/// * `framebuffer` - Raw pointer to the framebuffer memory
/// * `framebuffer_info` - Information about the framebuffer configuration
///
/// # Safety
///
/// The framebuffer pointer must point to valid memory with the dimensions
/// specified in framebuffer_info.
pub fn flanterm_init(framebuffer: *mut u32, framebuffer_info: FramebufferInfo) {
    let mut lock = FLANTERM.lock();
    *lock = Some(FlanConsole::new(framebuffer, framebuffer_info));
}

/// A terminal emulator implementation using the flanterm library.
///
/// Provides a high-level interface to the flanterm C library, implementing
/// a full terminal emulator with ANSI escape sequence support.
pub struct FlanConsole {
    /// Raw pointer to the flanterm context
    context: *mut flanterm_context,
}

unsafe impl Send for FlanConsole {}

impl FlanConsole {
    /// Creates a new FlanConsole instance.
    ///
    /// # Arguments
    ///
    /// * `framebuffer` - Raw pointer to the framebuffer memory
    /// * `framebuffer_info` - Information about the framebuffer configuration
    ///
    /// # Safety
    ///
    /// The framebuffer pointer must point to valid memory with the dimensions
    /// specified in framebuffer_info.
    pub fn new(framebuffer: *mut u32, framebuffer_info: FramebufferInfo) -> Self {
        let context = get_context(framebuffer, framebuffer_info);
        FlanConsole { context }
    }

    /// Internal print implementation that writes directly to the terminal.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to print to the terminal
    pub fn _print(&mut self, text: &str) {
        unsafe {
            flanterm_write(
                self.context,
                text.as_ptr() as *const i8,
                text.len(),
            );
        }
    }
}

impl Write for FlanConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self._print(s);
        Ok(())
    }
}

/// Creates and initializes a flanterm context.
///
/// # Arguments
///
/// * `framebuffer` - Raw pointer to the framebuffer memory
/// * `framebuffer_info` - Information about the framebuffer configuration
///
/// # Safety
///
/// The framebuffer pointer must point to valid memory that matches the dimensions
/// specified in framebuffer_info. The returned context must be properly managed
/// and freed when no longer needed.
fn get_context(framebuffer: *mut u32, framebuffer_info: FramebufferInfo) -> *mut flanterm_context {
    unsafe {
        flanterm_fb_init(
            None,
            None,
            framebuffer,
            framebuffer_info.width,
            framebuffer_info.height,
            framebuffer_info.pitch,
            framebuffer_info.red_mask_size,
            framebuffer_info.red_mask_shift,
            framebuffer_info.green_mask_size,
            framebuffer_info.green_mask_shift,
            framebuffer_info.blue_mask_size,
            framebuffer_info.blue_mask_shift,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            0,
            1,
            0,
            0,
            0,
        )
    }
}
