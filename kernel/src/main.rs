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

extern crate alloc;

use core::panic::PanicInfo;

use alloc::boxed::Box;
use bootloader_api::{
    BootInfo, BootloaderConfig,
    config::Mapping,
    entry_point,
    info::{FrameBuffer, FrameBufferInfo},
};
use gdt::init_gdt;
use interrupts::init_idt;
use memory::{BootInfoFrameAllocator, init_heap, paging};
use output::{Display, DisplayWriter, LineWriter, framebuffer::WrappedFrameBuffer};
use spin::mutex::Mutex;
use x86_64::VirtAddr;

pub static WRITER: Mutex<Option<LineWriter>> = Mutex::new(None);

/// Initializes the global display writer.
///
/// In the future, all fonts might need to be present in order to allow for selection
pub fn init_writer(framebuffer: &'static mut FrameBuffer, info: FrameBufferInfo) {
    let wrapped_buffer = Box::new(WrappedFrameBuffer::new(framebuffer));
    let wrapped_buffer: &'static mut WrappedFrameBuffer = Box::leak(wrapped_buffer);
    let display = Display::new(wrapped_buffer);
    let (font, width, height) = DisplayWriter::select_font_and_dimensions(info.height, info.width);
    let font = Box::leak(Box::new(font));
    let displaywriter = DisplayWriter::new(display, font, width, height);

    *WRITER.lock() = Some(LineWriter::new(displaywriter));
}

/// Global print! macro that writes to the framebuffer.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
            use $crate::WRITER;
            let mut lock = WRITER.lock();
            let writer = lock.as_mut().unwrap();
            let _ = write!(writer, $($arg)*);
            writer.flush();
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
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_regions) };

    let mut offset_allocator = unsafe {
        paging::init(VirtAddr::new(
            boot_info.physical_memory_offset.into_option().unwrap(),
        ))
    };

    unsafe {
        init_heap(&mut offset_allocator, &mut frame_allocator).expect("heap initialization failed");
    }

    let framebuffer_option = &mut boot_info.framebuffer;
    let framebuffer = framebuffer_option.as_mut().unwrap();
    let framebuffer_info = framebuffer.info();
    init_writer(framebuffer, framebuffer_info);

    for i in 0..100 {
        println!("Hello, world! {}", i);
    }

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
