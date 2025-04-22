use core::{fmt::Write, ptr};

use flanterm::sys::{flanterm_context, flanterm_fb_init, flanterm_write};
use spin::Mutex;

use super::framebuffer::FramebufferInfo;

pub static FLANTERM: Mutex<Option<FlanConsole>> = Mutex::new(None);

pub fn flanterm_init(framebuffer: *mut u32, framebuffer_info: FramebufferInfo) {
    let mut lock = FLANTERM.lock();
    *lock = Some(FlanConsole::new(framebuffer, framebuffer_info));
}

pub struct FlanConsole {
    context: *mut flanterm_context,
}

unsafe impl Send for FlanConsole {}

impl FlanConsole {
    pub fn new(framebuffer: *mut u32, framebuffer_info: FramebufferInfo) -> Self {
        let context = get_context(framebuffer, framebuffer_info);
        FlanConsole { context }
    }

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
