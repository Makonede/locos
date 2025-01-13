use core::{convert::Infallible, panic};

use bootloader_api::info::{FrameBuffer, PixelFormat};
use embedded_graphics::{pixelcolor::Rgb888, prelude::{DrawTarget, OriginDimensions, RgbColor}, Pixel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// Draw a pixel to the framebuffer in a certain position, accounting for alignment.
pub fn set_pixel_in(framebuffer: &mut FrameBuffer, position: Position, color: Color) {
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

/// Wrapper for framebuffer to implement DrawTarget. Only supports Rgb.
pub struct Display<'f> {
    framebuffer: &'f mut FrameBuffer,
}

impl<'f> Display<'f> {
    pub fn new(framebuffer: &'f mut FrameBuffer) -> Self {
        Self { framebuffer }
    }

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
}

impl<'f> DrawTarget for Display<'f> {
    type Color = Rgb888;

    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>> {
        for pixel in pixels.into_iter() {
            self.draw_pixel(pixel);
        }

        Ok(())
    }
} 

impl<'f> OriginDimensions for Display<'f> {
    fn size(&self) -> embedded_graphics::prelude::Size {
        let info = self.framebuffer.info();

        embedded_graphics::prelude::Size::new(info.width as u32, info.height as u32)
    }
}
