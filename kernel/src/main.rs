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

#![no_std]
#![no_main]
pub mod console;
pub mod framebuffer;

use core::{char, panic::PanicInfo};

use bootloader_api::{BootInfo, entry_point, info::FrameBufferInfo};
use bootloader_x86_64_common::logger::LockedLogger;
use conquer_once::spin::OnceCell;
use console::{DisplayWriter, ScreenChar};
use embedded_graphics::{mono_font::MonoTextStyle, pixelcolor::Rgb888};
use framebuffer::Display;

pub(crate) static _LOGGER: OnceCell<LockedLogger> = OnceCell::uninit();

pub(crate) fn _init_logger(framebuffer: &'static mut [u8], info: FrameBufferInfo) {
    let logger = _LOGGER.get_or_init(move || LockedLogger::new(framebuffer, info, true, false));
    log::set_logger(logger).expect("logger already set");
    log::set_max_level(log::LevelFilter::Trace);
    log::info!("Hello, World!");
}

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    let framebuffer_option = &mut boot_info.framebuffer;
    let framebuffer = framebuffer_option.as_mut().unwrap();
    let framebuffer_info = framebuffer.info();
    let mut display = Display::new(framebuffer);
    let binding = DisplayWriter::select_font(framebuffer_info.height, framebuffer_info.width);
    let mut displaywriter = DisplayWriter::new(
        &mut display,
        MonoTextStyle::new(&binding, Rgb888::new(255, 255, 255)),
    );
    displaywriter
        .write_string(
            0,
            0,
            &screen_chars!("Hello, world!", Rgb888::new(255, 255, 255)),
        )
        .expect("Failed to write string");
    loop {}
}

entry_point!(kernel_main);

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::error!("{info:?}");
    loop {}
}
