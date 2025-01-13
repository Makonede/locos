/*
Copyright © 2024 Mako and JayAndJef

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
pub mod framebuffer;

use core::panic::PanicInfo;

use bootloader_api::{entry_point, info::FrameBufferInfo, BootInfo};
use bootloader_x86_64_common::logger::LockedLogger;
use conquer_once::spin::OnceCell;

pub(crate) static LOGGER: OnceCell<LockedLogger> = OnceCell::uninit();

pub(crate) fn init_logger(framebuffer: &'static mut [u8], info: FrameBufferInfo) {
    let logger = LOGGER.get_or_init(move || LockedLogger::new(framebuffer, info, true, false));
    log::set_logger(logger).expect("logger already set");
    log::set_max_level(log::LevelFilter::Trace);
    log::info!("Hello, world!");
}

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    let framebuffer_optional = &mut boot_info.framebuffer;
    let deref_framebuffer = framebuffer_optional.as_mut();
    let framebuffer = deref_framebuffer.unwrap();
    let info_duplicate = framebuffer.info().clone();
    let raw_buffer = framebuffer.buffer_mut();
    init_logger(raw_buffer, info_duplicate);
    loop {}
}

entry_point!(kernel_main);

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
