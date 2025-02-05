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
pub mod linewriter;
pub mod serial;

use core::panic::PanicInfo;

use bootloader_api::{entry_point, info::{FrameBuffer, FrameBufferInfo}, BootInfo};
use conquer_once::spin::OnceCell;
use spin::mutex::Mutex;
use console::DisplayWriter;
use embedded_graphics::{mono_font::{MonoFont, MonoTextStyleBuilder}, pixelcolor::Rgb888};
use framebuffer::Display;
use linewriter::LineWriter;

pub static WRITER: Mutex<Option<LineWriter>> = Mutex::new(None);

static FONT: OnceCell<MonoFont> = OnceCell::uninit();

pub fn init_font(info: FrameBufferInfo) {
    FONT.init_once(|| DisplayWriter::select_font(info.height, info.width));
}

pub fn init_writer(framebuffer: &'static mut FrameBuffer, info: FrameBufferInfo) {
    let display = Display::new(framebuffer);
    init_font(info);
    let displaywriter = DisplayWriter::new(
        display,
        MonoTextStyleBuilder::new()
            .font(FONT.get().unwrap())
            .text_color(Rgb888::new(255, 255, 255))
            .background_color(Rgb888::new(0, 0, 0)) // kind of hacky fix for non-overlapping text
            .build(),
    );

    *WRITER.lock() = Some(LineWriter::new(displaywriter));
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
            let _ = write!(WRITER.lock().as_mut().unwrap(), $($arg)*);
        }
    };
}

#[macro_export]
macro_rules! println {
    () => {
        {
            $crate::print!("\n");
        }
    };
    ($($arg:tt)*) => {
        {
            $crate::print!("{}\n", format_args!($($arg)*));
        }
    };
}


fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    let framebuffer_option = &mut boot_info.framebuffer;
    let framebuffer = framebuffer_option.as_mut().unwrap();
    let framebuffer_info = framebuffer.info();
    init_writer(framebuffer, framebuffer_info);
    // WRITER.init_once(|| Mutex::new(linewriter));

    //init_writer(framebuffer);
    let lines = [
        "Linewriters are cool.",
        "Hello, World!",
        "Hello, Universe!",
        "This is a short line.",
        "This is a long line that should.................................................................... be wrapped around to the next line.",
    ];

    for line in lines.iter() {
        println!("{}", line);
    }
    panic!("something happened! panic!");
    loop {}
}

entry_point!(kernel_main);

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    serial_println!("{}", info);
    loop {}
}
