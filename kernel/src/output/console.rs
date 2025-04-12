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

    pub fn from_char(character: char) -> Self {
        Self {
            character,
            color: Rgb888::new(255, 255, 255),
        }
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
#[derive(Debug, Clone, Copy)]
pub struct Range {
    pub start_x: usize,
    pub start_y: usize,
    pub height: usize,
    pub width: usize,
}

impl Range {
    pub fn new(start_x: usize, start_y: usize, height: usize, width: usize) -> Self {
        Self {
            start_x,
            start_y,
            height,
            width,
        }
    }

    pub fn from_1drange(range: OneDRange, offset_y: usize) -> Self {
        Self {
            start_x: range.start,
            start_y: offset_y,
            height: 1,
            width: range.width,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OneDRange {
    pub start: usize,
    pub width: usize,
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

    /// Flushes the buffer at a range using a single draw operation
    pub fn flush_buffer_at_range(
        &mut self,
        range: OneDRange,
        offset_y: usize,
    ) -> Result<(), DisplayError> {
        if range.start > self.buffer_width
            || range.start + range.width > self.buffer_width
            || offset_y > self.buffer_height
        {
            return Err(DisplayError::OutOfBounds);
        }

        self.clear_range(Range::from_1drange(range, offset_y))?;

        let start = offset_y * self.buffer_width + range.start;
        let end = start + range.width;

        for (i, char) in self.buffer[start..end].iter().enumerate() {
            if char.character != ' ' {
                let mut style = self.text_style;
                style.text_color = Some(char.color);
                let x = (range.start + i) * self.text_style.font.character_size.width as usize;
                let y = offset_y * self.text_style.font.character_size.height as usize;
                let mut buf = [0u8; 4];
                Text::new(
                    char.character.encode_utf8(&mut buf),
                    Point::new(x as i32, y as i32),
                    style,
                )
                .draw(&mut self.display)
                .map_err(|_| DisplayError::DrawError)?;
            }
        }
        Ok(())
    }

    /// flushes entire buffer to the double buffer.
    /// 
    /// uses `flush_buffer_at_range`
    pub fn flush_entire_buffer(&mut self) -> Result<(), DisplayError> {
        let range = OneDRange {
            start: 0,
            width: self.buffer_width,
        };
        for y in 0..self.buffer_height {
            self.flush_buffer_at_range(range, y)?;
        }
        Ok(())
    }

    fn clear_range(&mut self, range: Range) -> Result<(), DisplayError> {
        // draw one big rectangle
        let x_coords = range.start_x * self.text_style.font.character_size.width as usize;
        let y_coords = range.start_y * self.text_style.font.character_size.height as usize;
        let rect = embedded_graphics::primitives::Rectangle::new(
            Point::new(x_coords as i32, y_coords as i32),
            Size::new(
                range.width as u32 * self.text_style.font.character_size.width,
                range.height as u32 * self.text_style.font.character_size.height,
            ),
        );

        rect.into_styled(
            embedded_graphics::primitives::PrimitiveStyleBuilder::new()
                .fill_color(Rgb888::new(0, 0, 0))
                .build(),
        )
        .draw(&mut self.display)
        .map_err(|_| DisplayError::DrawError)?;

        Ok(())
    }

    /// Writes a string to the buffer at the specified range
    /// 
    /// does not flush the buffer
    pub fn write_range(
        &mut self,
        offset_x: usize,
        offset_y: usize,
        characters: &[ScreenChar],
    ) -> Result<(), DisplayError> {
        if offset_x + characters.len() > self.buffer_width {
            return Err(DisplayError::OutOfBounds);
        }

        let start = offset_y * self.buffer_width + offset_x;
        let end = start + characters.len();
        self.buffer[start..end].copy_from_slice(characters);
        Ok(())
    }

    /// Flushes the buffer and writes a range of characters to the framebuffer
    pub fn write_and_flush_range(
        &mut self,
        offset_x: usize,
        offset_y: usize,
        characters: &[ScreenChar],
    ) -> Result<(), DisplayError> {
        self.write_range(offset_x, offset_y, characters)?;
        self.flush_buffer_at_range(OneDRange {
            start: offset_x,
            width: characters.len(),
        }, offset_y)
    }

    /// Get the character at a specific position in the buffer
    ///
    /// Panics if the coordinates are out of bounds.
    pub fn get_char(&self, offset_y: usize, offset_x: usize) -> ScreenChar {
        self.buffer[offset_y * self.buffer_width + offset_x]
    }

    /// Get char range at a specific range in the buffer
    pub fn get_char_range(&self, offset_y: usize, offset_x: usize, width: usize) -> &[ScreenChar] {
        let start = offset_y * self.buffer_width + offset_x;
        let end = start + width;
        &self.buffer[start..end]
    }

    /// Flushes the buffer to the framebuffer.
    pub fn flush(&mut self) {
        self.display.flush();
    }
}
