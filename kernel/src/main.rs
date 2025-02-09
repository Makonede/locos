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
#![feature(abi_x86_interrupt)]
pub mod gdt;
pub mod interrupts;
pub mod memory;
pub mod output;
pub mod serial;

use core::panic::PanicInfo;

use bootloader_api::{
    BootInfo, BootloaderConfig,
    config::Mapping,
    entry_point,
    info::{FrameBuffer, FrameBufferInfo},
};
use conquer_once::spin::OnceCell;
use embedded_graphics::{
    mono_font::{MonoFont, MonoTextStyleBuilder},
    pixelcolor::Rgb888,
};
use gdt::init_gdt;
use interrupts::init_idt;
use output::{Display, DisplayWriter, LineWriter};
use spin::mutex::Mutex;
use x86_64::{structures::paging::Translate, VirtAddr};

pub static WRITER: Mutex<Option<LineWriter>> = Mutex::new(None);

static FONT: OnceCell<MonoFont> = OnceCell::uninit();

pub fn init_font(info: FrameBufferInfo) {
    FONT.init_once(|| DisplayWriter::select_font(info.height, info.width));
}

/// Initializes the global display writer.
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

/// Global print! macro that writes to the framebuffer.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
            use $crate::WRITER;
            let _ = write!(WRITER.lock().as_mut().unwrap(), $($arg)*);
        }
    };
}

/// Global println! macro that writes to the framebuffer.
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
    init_gdt();
    init_idt();
    let framebuffer_option = &mut boot_info.framebuffer;
    let framebuffer = framebuffer_option.as_mut().unwrap();
    let framebuffer_info = framebuffer.info();
    init_writer(framebuffer, framebuffer_info);

    loop {}
}

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("{}", info);
    println!("{}", info);
    loop {}
}
