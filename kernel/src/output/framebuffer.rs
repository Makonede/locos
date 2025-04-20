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

use alloc::{slice, vec::Vec};
use embedded_graphics::{
    Pixel,
    pixelcolor::Rgb888,
    prelude::{Dimensions, DrawTarget, OriginDimensions, RgbColor},
    primitives::Rectangle,
};
use limine::framebuffer::{Framebuffer, MemoryModel};

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

#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    /// **bytes** per pixel
    pub bpp: usize,
    pub red_mask_size: u8,
    pub green_mask_size: u8,
    pub blue_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_shift: u8,
    pub blue_mask_shift: u8,
    pub memory_model: MemoryModel,
}

/// Converts the pointer to the start of the framebuffer to a mutable slice.
/// 
/// # Safety
/// The caller needs to make sure the frambuffer pointer and info point to a valid frambuffer.
pub unsafe fn get_buffer_from_framebuffer(framebuffer: *mut u8, info: FramebufferInfo) -> &'static mut [u8] {
    unsafe { slice::from_raw_parts_mut(framebuffer, info.height * info.pitch) }
}

pub fn get_info_from_frambuffer(framebuffer: &Framebuffer) -> FramebufferInfo {
    let pitch = framebuffer.pitch() as usize;
    let bpp = framebuffer.bpp() as usize;
    let width = framebuffer.width() as usize;
    let height = framebuffer.height() as usize;

    FramebufferInfo {
        width,
        height,
        pitch,
        bpp: bpp.div_ceil(8),  // Round up to nearest byte
        red_mask_size: framebuffer.red_mask_size(),
        green_mask_size: framebuffer.green_mask_size(),
        blue_mask_size: framebuffer.blue_mask_size(),
        red_mask_shift: framebuffer.red_mask_shift(),
        green_mask_shift: framebuffer.green_mask_shift(),
        blue_mask_shift: framebuffer.blue_mask_shift(),
        memory_model: framebuffer.memory_model(),
    }
}

pub struct WrappedFrameBuffer {
    framebuffer: *mut u8,
    pub info: FramebufferInfo,
    double_buffer: Vec<u8>,
}

/// must be used behind a mutex
unsafe impl Send for WrappedFrameBuffer {}

impl WrappedFrameBuffer {
    pub fn new(framebuffer: &mut Framebuffer) -> Self {
        let double_buffer = unsafe { get_buffer_from_framebuffer(framebuffer.addr(), get_info_from_frambuffer(framebuffer)) }
            .to_vec();
        Self {
            framebuffer: framebuffer.addr(),
            info: get_info_from_frambuffer(framebuffer),
            double_buffer,
        }
    }

    /// Flushes the double buffer to the framebuffer.
    pub fn flush(&mut self) {
        unsafe { get_buffer_from_framebuffer(self.framebuffer, self.info) }
            .copy_from_slice(&self.double_buffer);
    }

    /// Get a mutable reference to the double buffer.
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.double_buffer
    }
}

pub fn get_byte_offset(framebuffer: &WrappedFrameBuffer, position: Position) -> usize {
    let info = framebuffer.info;
    
    let line_offset = position.y * info.pitch;
    

    line_offset + position.x * info.bpp
}

/// Draw a pixel to the framebuffer in a certain position, accounting for alignment.
pub fn set_pixel_in(framebuffer: &mut WrappedFrameBuffer, position: Position, color: Color) {
    let info = framebuffer.info;

    let byte_offset = get_byte_offset(framebuffer, position);

    let pixel_buffer = &mut framebuffer.buffer_mut()[byte_offset..byte_offset + info.bpp];
    
    let red = ((color.red as u32) & ((1 << info.red_mask_size) - 1)) << info.red_mask_shift;
    let green = ((color.green as u32) & ((1 << info.green_mask_size) - 1)) << info.green_mask_shift;
    let blue = ((color.blue as u32) & ((1 << info.blue_mask_size) - 1)) << info.blue_mask_shift;
    let pixel = red | green | blue;

    for i in 0..info.bpp {
        pixel_buffer[i] = (pixel >> (8 * i)) as u8;
    }
}

/// Wrapper for framebuffer to implement DrawTarget. Only supports Rgb
/// in the form of `Rgb888` provided by `embedded_graphics`.
pub struct Display {
    framebuffer: WrappedFrameBuffer,
}

impl Display {
    pub fn new(framebuffer: WrappedFrameBuffer) -> Self {
        Self { framebuffer }
    }

    fn draw_pixel(&mut self, Pixel(coordinates, color): Pixel<Rgb888>) {
        let (width, height) = {
            let info = self.framebuffer.info;
            (info.width, info.height)
        };

        let (x, y) = {
            let c: (i32, i32) = coordinates.into();
            (c.0 as usize, c.1 as usize)
        };

        if (0..width).contains(&x) && (0..height).contains(&y) {
            let color = Color {
                red: color.r(),
                green: color.g(),
                blue: color.b(),
            };
            set_pixel_in(&mut self.framebuffer, Position { x, y }, color);
        };
    }

    pub fn fill_display(&mut self, color: Rgb888) {
        let info = self.framebuffer.info;
        let buffer = self.framebuffer.buffer_mut();

        for i in (0..buffer.len()).step_by(info.bpp) {
            let red = ((color.r() as u32) & ((1 << info.red_mask_size) - 1)) << info.red_mask_shift;
            let green = ((color.g() as u32) & ((1 << info.green_mask_size) - 1)) << info.green_mask_shift;
            let blue = ((color.b() as u32) & ((1 << info.blue_mask_size) - 1)) << info.blue_mask_shift;
            let pixel = red | green | blue;

            for j in 0..info.bpp {
                buffer[i + j] = (pixel >> (8 * j)) as u8;
            }
        }
    }

    /// flushes the double buffer to the framebuffer.
    pub fn flush(&mut self) {
        self.framebuffer.flush();
    }
}

/// Makes the framebuffer a DrawTarget for `embedded_graphics`.
impl DrawTarget for Display {
    type Color = Rgb888;

    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for pixel in pixels.into_iter() {
            self.draw_pixel(pixel);
        }

        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let area = area.intersection(&self.bounding_box());

        if area.size.width == 0 || area.size.height == 0 {
            return Ok(());
        }

        let info = self.framebuffer.info;
        let buffer = self.framebuffer.buffer_mut();

        let mut colors = colors.into_iter();
        let (start_x, start_y) = (area.top_left.x as usize, area.top_left.y as usize);

        let (width, height) = (area.size.width as usize, area.size.height as usize);

        for y in 0..height {
            let row_start = (start_y + y) * info.pitch + start_x * info.bpp;
            let row_end = row_start + width * info.bpp;

            for (i, color) in (row_start..row_end)
                .step_by(info.bpp)
                .zip(&mut colors)
            {
                let red = ((color.r() as u32) & ((1 << info.red_mask_size) - 1)) << info.red_mask_shift;
                let green = ((color.g() as u32) & ((1 << info.green_mask_size) - 1)) << info.green_mask_shift;
                let blue = ((color.b() as u32) & ((1 << info.blue_mask_size) - 1)) << info.blue_mask_shift;
                let pixel = red | green | blue;

                for j in 0..info.bpp {
                    buffer[i + j] = (pixel >> (8 * j)) as u8;
                }
            }
        }

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.fill_display(color);
        Ok(())
    }
}

/// Allows `embedded_graphics` to get the dimensions of the framebuffer.
impl OriginDimensions for Display {
    fn size(&self) -> embedded_graphics::prelude::Size {
        let info = self.framebuffer.info;
        embedded_graphics::prelude::Size::new(info.width as u32, info.height as u32)
    }
}
