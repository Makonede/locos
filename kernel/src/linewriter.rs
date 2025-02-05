use core::fmt::Write;

use embedded_graphics::pixelcolor::Rgb888;

use crate::console::{BUFFER_HEIGHT, BUFFER_WIDTH, DisplayError, DisplayWriter, ScreenChar};

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

    fn shift_buffer_up(&mut self) -> Result<(), DisplayError> {
        for y in 0..BUFFER_HEIGHT - 1 {
            for x in 0..BUFFER_WIDTH {
                self.displaywriter
                    .write_char(y, x, self.displaywriter.buffer[y + 1][x])?;
            }
        }

        for i in 0..BUFFER_WIDTH {
            self.displaywriter.write_char(
                BUFFER_HEIGHT - 1,
                i,
                ScreenChar::new(' ', Rgb888::new(255, 255, 255)),
            )?;
        }

        Ok(())
    }

    pub fn write(&mut self, string: &str) -> Result<(), DisplayError> {
        for c in string.chars() {
            if c == '\n' || self.cursor_position >= BUFFER_WIDTH {
                self.shift_buffer_up()?;
                self.cursor_position = 0;
                continue;
            }
            self.displaywriter.write_char(
                BUFFER_HEIGHT - 1,
                self.cursor_position,
                ScreenChar::new(c, Rgb888::new(255, 255, 255)),
            )?;
            self.cursor_position += 1;
        }

        Ok(())
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
