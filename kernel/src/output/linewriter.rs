use core::fmt::Write;

use embedded_graphics::pixelcolor::Rgb888;

use crate::{output::console::{DisplayError, DisplayWriter, ScreenChar}, serial_println};

/// Simple class that always outputs to the last line of the screen and always uses white text.
/// NOTE: This is a very simple implementation that does not handle scrolling, and might get merged into DisplayWriter in the future.
pub struct LineWriter<'a> {
    cursor_position: usize,
    displaywriter: DisplayWriter<'a>,
}

impl<'a> LineWriter<'a> {
    pub fn new(displaywriter: DisplayWriter<'a>) -> Self {
        Self {
            cursor_position: 0,
            displaywriter,
        }
    }

    /// Shifts the buffer up by one line, clearing the last.
    fn shift_buffer_up(&mut self) -> Result<(), DisplayError> {
        for y in 0..self.displaywriter.buffer_height - 1 {
            for x in 0..self.displaywriter.buffer_width {
                self.displaywriter
                    .write_char(y, x, self.displaywriter.buffer[(y + 1) * self.displaywriter.buffer_width + x])?;
            }
        }

        for i in 0..self.displaywriter.buffer_width {
            self.displaywriter.write_char(
                self.displaywriter.buffer_height - 1,
                i,
                ScreenChar::new(' ', Rgb888::new(255, 255, 255)),
            )?;
        }

        Ok(())
    }

    /// Writes a string to the last line of the screen, shifting the buffer up if necessary.
    pub fn write(&mut self, string: &str) -> Result<(), DisplayError> {
        for c in string.chars() {
            if c == '\n' || self.cursor_position >= self.displaywriter.buffer_width {
                self.shift_buffer_up()?;
                self.cursor_position = 0;
                continue;
            }
            self.displaywriter.write_char(
                self.displaywriter.buffer_height - 1,
                self.cursor_position,
                ScreenChar::new(c, Rgb888::new(255, 255, 255)),
            )?;
            self.cursor_position += 1;
        }

        Ok(())
    }

    /// FLushes the buffer to the screen.
    pub fn flush(&mut self) {
        self.displaywriter.flush()
    }
}

impl Write for LineWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        match self.write(s) {
            Ok(_) => Ok(()),
            Err(_) => Err(core::fmt::Error),
        }
    }
}
