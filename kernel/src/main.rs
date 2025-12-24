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
#![feature(custom_test_frameworks)]
#![test_runner(crate::testing::test_runner)]
#![reexport_test_harness_main = "test_main"]

pub mod gdt;
pub mod interrupts;
pub mod memory;
pub mod meta;
pub mod output;
pub mod pci;
pub mod ps2;
pub mod serial;
pub mod shell;
pub mod syscall;
pub mod tasks;
pub mod testing;

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
        RsdpRequest, StackSizeRequest,
    },
};
use memory::{
    init_frame_allocator, init_heap, init_page_allocator,
    paging::{self, fill_page_list},
};
use output::{flanterm_init, framebuffer::get_info_from_frambuffer};
use x86_64::{VirtAddr, registers::debug};


#[cfg(not(test))]
use crate::{
    interrupts::apic::LAPIC_TIMER_VECTOR,
    tasks::scheduler::{kcreate_task, kinit_multitasking},
};
#[cfg(not(test))]
use meta::tprint_welcome;

pub const STACK_SIZE: u64 = 0x100000;

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

    #[allow(unused_variables)]
    for entry in memory_regions {
        debug!(
            "Memory region: base = {:#x} - {:#x}, usable = {:?}",
            entry.base + physical_memory_offset,
            entry.base + physical_memory_offset + entry.length,
            entry.entry_type == EntryType::USABLE,
        );
    }

    debug!("Physical memory offset: {:#x}", physical_memory_offset);
    unsafe { fill_page_list(memory_regions, physical_memory_offset as usize) };
    debug!("Filling page list done");
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

    #[allow(unused_variables)]
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

    syscall::init_syscall();

    ps2::init().expect("failed to initialize PS/2 subsystem");

    pci::init_pci(rsdp_addr).expect("failed to initialize PCIe subsystem");

    #[cfg(test)]
    {
        // Clear console and run tests before starting kernel tasks
        print!("\x1B[2J\x1B[H"); // Clear screen and move cursor to top
        test_main();
    }

    #[cfg(not(test))]
    {
        use crate::shell::task::locos_shell;
        use crate::tasks::scheduler::ucreate_task;

        const TEST_PROGRAM: &[u8] = &[
            0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00,  // mov rax, 1 (sys_write)
            0x48, 0xc7, 0xc7, 0x01, 0x00, 0x00, 0x00,  // mov rdi, 1 (stdout)
            0x48, 0x8d, 0x35, 0x19, 0x00, 0x00, 0x00,  // lea rsi, [rip+25] (message)
            0x48, 0xc7, 0xc2, 0x16, 0x00, 0x00, 0x00,  // mov rdx, 22 (length)
            0x0f, 0x05,                                // syscall
            0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00,  // mov rax, 0 (sys_exit)
            0x48, 0xc7, 0xc7, 0x00, 0x00, 0x00, 0x00,  // mov rdi, 0 (exit code)
            0x0f, 0x05,                                // syscall
            // "Hello from userspace!\n"
            0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x66, 0x72,
            0x6f, 0x6d, 0x20, 0x75, 0x73, 0x65, 0x72, 0x73,
            0x70, 0x61, 0x63, 0x65, 0x21, 0x0a,
        ];

        kcreate_task(tprint_welcome, "print welcome message");
        kcreate_task(locos_shell, "locos shell");
        
        if let Err(e) = ucreate_task(VirtAddr::new(0x400000), Some(TEST_PROGRAM), "test_userspace") {
            error!("Failed to create test userspace task: {}", e);
        }
        
        kinit_multitasking();

        x86_64::instructions::interrupts::enable();

        unsafe {
            core::arch::asm!("int {}", const LAPIC_TIMER_VECTOR);
        }

        pci::nvme::init();
    }

    hcf();
}

#[used]
#[unsafe(link_section = ".requests")]
pub static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static STACK_SIZE_REQUEST: StackSizeRequest = StackSizeRequest::new().with_size(STACK_SIZE);

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

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("{}", info);
    hcf();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use crate::testing::{QemuExitCode, exit_qemu};

    serial_println!("[failed]");
    serial_println!("Error: {}", info);
    exit_qemu(QemuExitCode::Failed);
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

#[test_case]
fn trivial_assertion() {
    let x = 1;
    assert_eq!(1, x);
}

#[test_case]
fn test_basic_arithmetic() {
    let a = 2;
    let b = 2;
    assert_eq!(a + b, 4);

    let c = 10;
    let d = 5;
    assert_eq!(c - d, 5);
}
