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

use core::convert::Infallible;

use alloc::vec::Vec;
use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use embedded_graphics::{Pixel, pixelcolor::Rgb888, prelude::{
    DrawTarget, OriginDimensions, RgbColor
}};

/// Represents a position on the framebuffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: usize,
    pub y: usize,
}

/// Represents a color in RGB format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}


pub struct WrappedFrameBuffer<'a> {
    framebuffer: &'a mut FrameBuffer,
    double_buffer: Vec<u8>,
}

impl<'a> WrappedFrameBuffer<'a> {
    pub fn new(framebuffer: &'a mut FrameBuffer) -> Self {
        let double_buffer = framebuffer.buffer().to_vec();
        Self { framebuffer, double_buffer }
    }

    /// Flushes the double buffer to the framebuffer.
    pub fn flush(&mut self) {
        self.framebuffer.buffer_mut().copy_from_slice(&self.double_buffer);
    }

    /// Get a mutable referance to the double buffer.
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.double_buffer
    }

    /// Get the intermal framebuffer info.
    pub fn info(&self) -> FrameBufferInfo {
        self.framebuffer.info()
    }
}

/// Draw a pixel to the framebuffer in a certain position, accounting for alignment.
pub fn set_pixel_in(framebuffer: &mut WrappedFrameBuffer, position: Position, color: Color) {
    let info = framebuffer.info();

    let byte_offset = {
        let line_offset = position.y * info.stride;
        let pixel_offset = line_offset + position.x;

        pixel_offset * info.bytes_per_pixel
    };

    let pixel_buffer = &mut framebuffer.buffer_mut()[byte_offset..byte_offset+4];
    match info.pixel_format {
        PixelFormat::Rgb => {
            pixel_buffer[0] = color.red;
            pixel_buffer[1] = color.green;
            pixel_buffer[2] = color.blue;
        }
        PixelFormat::Bgr => {
            pixel_buffer[2] = color.red;
            pixel_buffer[1] = color.green;
            pixel_buffer[0] = color.blue;
        },
        PixelFormat::U8 => {
            pixel_buffer[0] = color.red / 3 + color.green / 3 + color.blue / 3;
        },
        other => panic!("unknown pixel format {other:?}"),
    }
}

/// Wrapper for framebuffer to implement DrawTarget. Only supports Rgb
/// in the form of `Rgb888` provided by `embedded_graphics`.
pub struct Display<'a> { framebuffer: &'a mut WrappedFrameBuffer<'a> }

impl<'a> Display<'a> {
    pub fn new(framebuffer: &'a mut WrappedFrameBuffer<'a>) -> Self { Self { framebuffer } }

    fn draw_pixel(&mut self, Pixel(coordinates, color): Pixel<Rgb888>) {
        let (width, height) = {
            let info =  self.framebuffer.info();
            (info.width, info.height)
        };

        let (x, y) = {
            let c: (i32, i32) = coordinates.into();
            (c.0 as usize, c.1 as usize)
        };

        if (0..width).contains(&x) && (0..height).contains(&y) {
            let color = Color { red: color.r(), green: color.g(), blue: color.b() };
            set_pixel_in(self.framebuffer, Position { x, y }, color);
        };
    }

    pub fn fill_display(&mut self, color: Rgb888) {
        let color = Color { red: color.r(), green: color.g(), blue: color.b() };
        let info = self.framebuffer.info();
        let width = info.width;
        let height = info.height;

        for y in 0..height {
            for x in 0..width {
                set_pixel_in(self.framebuffer, Position { x, y }, color);
            }
        }
    }
    
    /// flushes the double buffer to the framebuffer.
    pub fn flush(&mut self) {
        self.framebuffer.flush();
    }
}

/// Makes the framebuffer a DrawTarget for `embedded_graphics`.
impl DrawTarget for Display<'_> {
    type Color = Rgb888;

    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>> {
        for pixel in pixels.into_iter() { self.draw_pixel(pixel); }

        Ok(())
    }
} 

/// Allows `embedded_graphics` to get the dimensions of the framebuffer.
impl OriginDimensions for Display<'_> {
    fn size(&self) -> embedded_graphics::prelude::Size {
        let info = self.framebuffer.info();
        embedded_graphics::prelude::Size::new(info.width as u32, info.height as u32)
    }
}
