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
use alloc::vec;
use alloc::vec::Vec;
use embedded_graphics::{
    Drawable,
    mono_font::{
        MonoFont, MonoTextStyle,
        ascii::{FONT_6X10, FONT_8X13, FONT_10X20},
    },
    pixelcolor::Rgb888,
    prelude::{Point, Primitive, Size},
    text::Text,
};

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
    ($text:expr, $color:expr) => {{
        const LEN: usize = $text.len();
        let mut chars = [ScreenChar::new(' ', $color); LEN];
        let mut i = 0;
        for c in $text.chars() {
            chars[i] = ScreenChar::new(c, $color);
            i += 1;
        }
        chars
    }};
}

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
    pub buffer: Vec<ScreenChar>,
    pub buffer_width: usize,
    pub buffer_height: usize,
    text_style: MonoTextStyle<'a, Rgb888>,
}

impl<'a> DisplayWriter<'a> {
    pub fn new(display: Display<'a>, font: &'a MonoFont<'a>, width: usize, height: usize) -> Self {
        let default_char = ScreenChar::new(' ', Rgb888::new(255, 255, 255));
        let buffer = vec![default_char; width * height];

        Self {
            display,
            buffer,
            text_style: MonoTextStyle::new(&font, Rgb888::new(255, 255, 255)),
            buffer_width: width,
            buffer_height: height,
        }
    }

    /// Calculates the default buffer dimensions based on the display size and font.
    fn calculate_buffer_dimensions(
        display_width: usize,
        display_height: usize,
        font: &MonoFont,
    ) -> (usize, usize) {
        let buffer_width = display_width / font.character_size.width as usize;
        let buffer_height = display_height / font.character_size.height as usize;
        (buffer_width, buffer_height)
    }

    /// Selects a font based on the display height and width.
    /// Returns a static 'MonoFont'. Consider using a `OnceCell` or similar
    /// to store the font.
    pub fn select_font_and_dimensions(
        display_height: usize,
        display_width: usize,
    ) -> (MonoFont<'static>, usize, usize) {
        for font in [FONT_10X20, FONT_8X13, FONT_6X10] {
            let (width, height) =
                Self::calculate_buffer_dimensions(display_width, display_height, &font);
            if width > 0 && height > 0 {
                return (font, width, height);
            }
        }
        panic!("screen too small for any font");
    }

    /// Flushes the buffer at point to the double buffer.
    ///
    /// If the character is a space, fill it with an empty rectangle
    pub fn flush_buffer_at_point(
        &mut self,
        offset_y: usize,
        offset_x: usize,
    ) -> Result<(), DisplayError> {
        if offset_y > self.buffer_height || offset_x > self.buffer_width {
            return Err(DisplayError::OutOfBounds);
        }

        let buffer_char = self.buffer[offset_y * self.buffer_width + offset_x];

        // clear the space
        self.clear_cell(offset_x, offset_y)?;

        // if the character is a space, we don't need to do anything else
        if buffer_char.character == ' ' {
            return Ok(());
        }

        let style = {
            let mut self_style = self.text_style;
            self_style.text_color = Some(buffer_char.color);
            self_style
        };

        let x_coords = offset_x * self.text_style.font.character_size.width as usize;
        let y_coords = offset_y * self.text_style.font.character_size.height as usize;
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

    /// Clears a cell at the specified coordinates by filling it with a black rectangle.
    fn clear_cell(&mut self, offset_x: usize, offset_y: usize) -> Result<(), DisplayError> {
        let x_coords = offset_x * self.text_style.font.character_size.width as usize;
        let y_coords = offset_y * self.text_style.font.character_size.height as usize;
        let rect = embedded_graphics::primitives::Rectangle::new(
            Point::new(x_coords as i32, y_coords as i32),
            Size::new(
                self.text_style.font.character_size.width,
                self.text_style.font.character_size.height,
            ),
        );
        rect.into_styled(
            embedded_graphics::primitives::PrimitiveStyleBuilder::new()
                .fill_color(Rgb888::new(0, 0, 0))
                .build(),
        )
        .draw(&mut self.display)
        .map_err(|_| DisplayError::DrawError)
    }

    /// Writes a character to the buffer at the specified coordinates.
    pub fn write_char(
        &mut self,
        offset_y: usize,
        offset_x: usize,
        character: ScreenChar,
    ) -> Result<(), DisplayError> {
        self.buffer[offset_y * self.buffer_width + offset_x] = character;
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
            if x >= self.buffer_width {
                y += 1;
                x = 0;
            }
            if y >= self.buffer_height {
                return Err(DisplayError::OutOfBounds);
            }
            self.write_char(y, x, *c)?;
            x += 1;
        }
        Ok(())
    }
}
