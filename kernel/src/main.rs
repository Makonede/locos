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
pub mod meta;
pub mod output;
pub mod serial;
pub mod tasks;

extern crate alloc;

use core::{arch::asm, panic::PanicInfo};

use alloc::vec::Vec;
use gdt::init_gdt;
use interrupts::{init_idt, setup_apic};
use limine::{
    BaseRevision,
    memory_map::EntryType,
    request::{
        FramebufferRequest, HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker,
        RsdpRequest,
    },
};
use memory::{init_frame_allocator, init_heap, init_page_allocator, paging};
use meta::print_welcome;
use output::{flanterm_init, framebuffer::get_info_from_frambuffer};
use x86_64::{VirtAddr, registers::debug};

#[unsafe(no_mangle)]
unsafe extern "C" fn kernel_main() -> ! {
    assert!(BASE_REVISION.is_supported());
    init_gdt();
    init_idt();

    let memory_regions = MEMORY_MAP_REQUEST
        .get_response()
        .expect("memory map request failed")
        .entries();

    let physical_memory_offset = HHDM_REQUEST
        .get_response()
        .expect("Hhdm request failed")
        .offset();

    unsafe { init_frame_allocator(memory_regions, physical_memory_offset) };

    unsafe { paging::init(VirtAddr::new(physical_memory_offset)) };

    unsafe {
        init_heap().expect("heap initialization failed");
    }

    // sum all usable memory regions
    let usable_regions_sum = memory_regions
        .iter()
        .filter(|entry| entry.entry_type == EntryType::USABLE)
        .map(|entry| entry.length)
        .sum::<u64>();

    let usable_regions = memory_regions
        .iter()
        .filter(|entry| entry.entry_type == EntryType::USABLE)
        .map(|entry| entry.length)
        .collect::<Vec<_>>();

    debug!(
        "Total usable memory: {} bytes ({:.2} GiB) spread over {:?} regions",
        usable_regions_sum,
        usable_regions_sum as f64 / (1024.0 * 1024.0 * 1024.0),
        usable_regions,
    );
    init_page_allocator(usable_regions_sum);

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
        get_info_from_frambuffer(&framebuffer),
    );

    let rsdp_addr = RSDP_REQUEST
        .get_response()
        .expect("RSDP request failed")
        .address();

    unsafe { setup_apic(rsdp_addr) };
    x86_64::instructions::interrupts::enable();

    print_welcome();

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
#[unsafe(link_section = ".requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();
#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("{}", info);
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
