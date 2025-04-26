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

use limine::framebuffer::{Framebuffer, MemoryModel};

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