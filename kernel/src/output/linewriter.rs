use core::fmt::Write;

use alloc::vec;
use alloc::vec::Vec;
use embedded_graphics::pixelcolor::Rgb888;

use crate::output::console::{DisplayError, DisplayWriter, ScreenChar};

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
        let mut characters = Vec::new();
        for y in 0..self.displaywriter.buffer_height - 1 {
            characters.extend_from_slice(self.displaywriter.get_char_range(
                y + 1,
                0,
                self.displaywriter.buffer_width,
            ));

            self.displaywriter.write_range(0, y, &characters)?;
            characters.clear();
        }

        let blank_characters =
            vec![ScreenChar::new(' ', Rgb888::new(255, 255, 255)); self.displaywriter.buffer_width];
        self.displaywriter.write_range(
            0,
            self.displaywriter.buffer_height - 1,
            &blank_characters,
        )?;

        self.displaywriter.flush_entire_buffer()?;

        Ok(())
    }

    /// Writes a string to the last line of the screen, shifting the buffer up if necessary.
    pub fn write(&mut self, string: &str) -> Result<(), DisplayError> {
        let mut curr_chars: Vec<ScreenChar> = Vec::new();

        for c in string.chars() {
            if c == '\n' || self.cursor_position >= self.displaywriter.buffer_width {
                if !curr_chars.is_empty() {
                    self.displaywriter.write_and_flush_range(
                        self.cursor_position - curr_chars.len(),
                        self.displaywriter.buffer_height - 1,
                        &curr_chars,
                    )?;
                }

                self.shift_buffer_up()?;
                self.cursor_position = 0;
                curr_chars.clear();
                continue;
            }

            curr_chars.push(ScreenChar::from_char(c));
            self.cursor_position += 1;
        }

        if !curr_chars.is_empty() {
            self.displaywriter.write_and_flush_range(
                self.cursor_position - curr_chars.len(),
                self.displaywriter.buffer_height - 1,
                &curr_chars,
            )?;
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
