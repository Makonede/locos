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

use gdt::init_gdt;
use interrupts::init_idt;
use limine::{
    BaseRevision,
    request::{
        FramebufferRequest, HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker,
    },
};
use memory::{BootInfoFrameAllocator, init_heap, paging};
use output::{flanterm_init, framebuffer::get_info_from_frambuffer};
use x86_64::VirtAddr;


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
    let framebuffer = framebuffer_response
        .framebuffers()
        .next()
        .expect("framebuffer not found");

    if framebuffer.bpp() % 8 != 0 {
        panic!("Framebuffer bpp is not a multiple of 8");
    }
    
    flanterm_init(
        framebuffer.addr() as *mut u32,
        get_info_from_frambuffer(&framebuffer)
    );

    for i in 0..100 {
        println!("Hello, world! {}", i);
    }

    info!("Hello, world!");
    debug!("Hello, world!");
    warn!("Hello, world!");
    error!("Hello, world!");
    trace!("Hello, world!");

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
