use core::fmt::Write;

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
        // Calculate dimensions
        let line_width = self.displaywriter.buffer_width;
        let total_lines = self.displaywriter.buffer_height;
        
        // Move all lines up at once using the underlying buffer
        self.displaywriter.buffer.copy_within(
            line_width..(total_lines * line_width),
            0
        );

        // Clear the last line
        let blank_line_start = (total_lines - 1) * line_width;
        let blank_line_end = total_lines * line_width;
        self.displaywriter.buffer[blank_line_start..blank_line_end]
            .fill(ScreenChar::new(' ', Rgb888::new(255, 255, 255)));
        
        // Flush the changes
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
