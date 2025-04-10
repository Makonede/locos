use core::ptr;

use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use flanterm::sys::{flanterm_context, flanterm_fb_init, flanterm_write};

pub struct Flanterm {
    pub context: *mut flanterm_context,
}

impl Flanterm {
    /// Creates a new Flanterm instance with the given framebuffer info and start address.
    ///
    /// # Safety
    /// make sure the framebuffer start is valid
    pub unsafe fn new(context: *mut flanterm_context) -> Self {
        Self { context }
    }

    pub fn write(&mut self, text: &str) {
        unsafe { flanterm_write(self.context, text.as_ptr() as *const i8, text.len()) };
    }
}

/// initializes flanterm with default behavior
/// 
/// # Safety
/// Must be made sure that the framebuffer start is valid
pub unsafe fn init_flanterm(info: FrameBufferInfo, start: *mut u32) -> Flanterm {
    let (
        red_mask_size,
        red_mask_shift,
        green_mask_size,
        green_mask_shift,
        blue_mask_size,
        blue_mask_shift,
    ) = match info.pixel_format {
        PixelFormat::Bgr => (8, 16, 8, 8, 8, 0),
        PixelFormat::Rgb => (8, 0, 8, 8, 8, 16),
        _ => panic!("Unsupported pixel format"),
    };

    unsafe {
        Flanterm::new(
            flanterm_fb_init(
                None,
                None,
                start,
                info.width,
                info.height,
                info.stride,
                red_mask_size,
                red_mask_shift,
                green_mask_size,
                green_mask_shift,
                blue_mask_size,
                blue_mask_shift,
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
        )
    }
}
