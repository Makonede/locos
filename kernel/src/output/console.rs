/*
Copyright © 2024–2025 Mako and JayAndJef

This file is part of locOS.

locOS is free software: you can redistribute it and/or modify it under the terms of the GNU General
Public License as published by the Free Software Foundation, either version 3 of the License, or (at
your option) any later version.

locOS is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the
implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public
License for more details.

You should have received a copy of the GNU General Public License along with locOS. If not, see
<https://www.gnu.org/licenses/>.
*/

use crate::output::framebuffer::Display;
use embedded_graphics::{Drawable, mono_font::{MonoFont, MonoTextStyle, ascii::{
    FONT_6X10, FONT_8X13, FONT_10X20
}}, pixelcolor::Rgb888, prelude::Point, text::Text};

/// Represents a character and its color for console display.
///
/// Used for framebuffer output.
#[derive(Debug, Clone, Copy)]
pub struct ScreenChar {
    pub character: char,
    pub color: Rgb888,
}

impl ScreenChar {
    pub fn new(character: char, color: Rgb888) -> Self {
        Self { character, color }
    }
}

/// Creates an array of `ScreenChar` from a string slice with a specified color.
///
/// This macro takes a string literal and a color, and returns a fixed-size array
/// of `ScreenChar` structs, where each character in the string is converted into
/// a `ScreenChar` with the specified color.
///
/// # Arguments
///
/// * `$text` - A string literal (`&str`) to convert into `ScreenChar` array.
/// * `$color` - An `Rgb888` color to apply to all characters.
#[macro_export]
macro_rules! screen_chars {
    ($text:expr, $color:expr) => {
        {
            const LEN: usize = $text.len();
            let mut chars = [ScreenChar::new(' ', $color); LEN];
            let mut i = 0;
            for c in $text.chars() {
                chars[i] = ScreenChar::new(c, $color);
                i += 1;
            }
            chars
        }
    }
}

pub const BUFFER_WIDTH: usize = 80;
pub const BUFFER_HEIGHT: usize = 25;

/// Represents errors that can occur during display operations.
#[derive(Debug, Clone, Copy)]
pub enum DisplayError {
    OutOfBounds,
    DrawError,
}

/// Manages writing characters to the display buffer and rendering them.
///
/// This struct provides methods for writing characters and strings
/// to an in-memory buffer, and then rendering that buffer to the framebuffer. It
/// uses the `embedded-graphics` crate for drawing operations.
pub struct DisplayWriter<'a> {
    display: Display<'a>,
    pub buffer: [[ScreenChar; BUFFER_WIDTH]; BUFFER_HEIGHT],
    text_style: MonoTextStyle<'a, Rgb888>,
}

impl<'a> DisplayWriter<'a> {
    pub fn new(display: Display<'a>, text_style: MonoTextStyle<'a, Rgb888>) -> Self {
        let default_char = ScreenChar {
            character: ' ',
            color: Rgb888::new(255, 255, 255),
        };

        Self {
            display,
            buffer: [[default_char; BUFFER_WIDTH]; BUFFER_HEIGHT],
            text_style,
        }
    }

    /// Selects a font based on the display height and width.
    /// Returns a static 'MonoFont'. Consider using a `OnceCell` or similar
    /// to store the font.
    pub fn select_font(height: usize, width: usize) -> MonoFont<'static> {
        let char_width = width / BUFFER_WIDTH;
        let char_height = height / BUFFER_HEIGHT;

        if char_width >= 10 && char_height >= 20 {
            FONT_10X20
        } else if char_width >= 8 && char_height >= 13 {
            FONT_8X13
        } else if char_width >= 6 && char_height >= 10 {
            FONT_6X10
        } else {
            panic!("screen too small");
        }
    }

    /// Flushes the buffer at point to the double buffer.
    pub fn flush_buffer_at_point(
        &mut self,
        offset_y: usize,
        offset_x: usize,
    ) -> Result<(), DisplayError> {
        if offset_y > BUFFER_HEIGHT || offset_x > BUFFER_WIDTH {
            return Err(DisplayError::OutOfBounds);
        }

        let buffer_char = self.buffer[offset_y][offset_x];
        let style = {
            let mut self_style = self.text_style;
            self_style.text_color = Some(buffer_char.color);
            self_style
        };

        let x_coords = offset_x * self.text_style.font.character_size.width as usize;
        let y_coords = (offset_y * self.text_style.font.character_size.height as usize)
            + self.text_style.font.character_size.height as usize;
        let mut buf = [0u8; 4];
        Text::new(
            buffer_char.character.encode_utf8(&mut buf),
            Point::new(x_coords as i32, y_coords as i32),
            style,
        )
        .draw(&mut self.display)
        .map_err(|_| DisplayError::DrawError)?;

        Ok(())
    }

    /// Writes a character to the buffer at the specified coordinates.
    pub fn write_char(
        &mut self,
        offset_y: usize,
        offset_x: usize,
        character: ScreenChar,
    ) -> Result<(), DisplayError> {
        self.buffer[offset_y][offset_x] = character;
        self.flush_buffer_at_point(offset_y, offset_x)?;
        Ok(())
    }

    /// Flushes the buffer to the framebuffer.
    pub fn flush(&mut self) {
        self.display.flush();
    }

    /// Writes a string to the buffer at the specified coordinates, wrapping if necessary.
    #[deprecated(note = "Use LineWriter as a wrapper around DisplayWriter instead.")]
    pub fn write_string(
        &mut self,
        offset_y: usize,
        offset_x: usize,
        characters: &[ScreenChar],
    ) -> Result<(), DisplayError> {
        let mut y = offset_y;
        let mut x = offset_x;

        for c in characters.iter() {
            if x >= BUFFER_WIDTH {
                y += 1;
                x = 0;
            }
            if y >= BUFFER_HEIGHT {
                return Err(DisplayError::OutOfBounds);
            }
            self.write_char(y, x, *c)?;
            x += 1;
        }
        Ok(())
    }
}
