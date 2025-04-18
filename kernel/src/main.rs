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

use core::{arch::asm, panic::PanicInfo};

use alloc::boxed::Box;
use gdt::init_gdt;
use interrupts::init_idt;
use limine::{
    BaseRevision,
    framebuffer::Framebuffer,
    request::{
        FramebufferRequest, HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker,
    },
};
use memory::{BootInfoFrameAllocator, init_heap, paging};
use output::{Display, DisplayWriter, LineWriter, framebuffer::WrappedFrameBuffer};
use spin::mutex::Mutex;
use x86_64::VirtAddr;

pub static WRITER: Mutex<Option<LineWriter>> = Mutex::new(None);

/// Initializes the global display writer.
///
/// In the future, all fonts might need to be present in order to allow for selection
pub fn init_writer(framebuffer: &mut Framebuffer) {
    let wrapped_buffer = WrappedFrameBuffer::new(framebuffer);
    let display = Display::new(wrapped_buffer);
    let (font, width, height) =
        DisplayWriter::select_font_and_dimensions(framebuffer.height() as usize, framebuffer.width() as usize);
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

#[unsafe(no_mangle)]
unsafe extern "C" fn kernel_main() -> ! {
    assert!(BASE_REVISION.is_supported());
    init_gdt();
    init_idt();

    let memory_regions = MEMORY_MAP_REQUEST
        .get_response()
        .expect("memory map request failed")
        .entries();
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(memory_regions) };

    let physical_memory_offset = HHDM_REQUEST
        .get_response()
        .expect("Hhdm request failed")
        .offset();
    let mut offset_allocator = unsafe { paging::init(VirtAddr::new(physical_memory_offset)) };

    unsafe {
        init_heap(&mut offset_allocator, &mut frame_allocator).expect("heap initialization failed");
    }

    let framebuffer_response = FRAMEBUFFER_REQUEST
        .get_response()
        .expect("framebuffer request failed");
    let mut framebuffer = framebuffer_response
        .framebuffers()
        .next()
        .expect("framebuffer not found");

    if framebuffer.bpp() % 8 != 0 {
        panic!("Framebuffer bpp is not a multiple of 8");
    }
    
    init_writer(&mut framebuffer);

    for i in 0..100 {
        println!("Hello, world! {}", i);
    }

    hcf();
}

#[used]
#[unsafe(link_section = ".requests")]
pub static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();
#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("{}", info);
    println!("{}", info);
    hcf();
}

fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            asm!("hlt");
        }
    }
}
