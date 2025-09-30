//! PS/2 keyboard driver for the kernel.
//!
//! This module provides PS/2 keyboard support including:
//! - Low-level PS/2 controller communication
//! - Keyboard interrupt handling
//! - Scancode to keycode translation
//! - Keyboard state management
//! - Input buffering

pub mod keyboard;

use crate::{info, warn};
use x86_64::instructions::port::Port;

/// PS/2 controller data port (read/write)
const PS2_DATA_PORT: u16 = 0x60;
/// PS/2 controller command/status port
const PS2_COMMAND_PORT: u16 = 0x64;

/// PS/2 controller status register bits
pub mod status_bits {
    /// Output buffer full (data available to read)
    pub const OUTPUT_BUFFER_FULL: u8 = 0x01;
    /// Input buffer full (controller busy)
    pub const INPUT_BUFFER_FULL: u8 = 0x02;
    /// System flag
    pub const SYSTEM_FLAG: u8 = 0x04;
    /// Command/data flag (0 = data, 1 = command)
    pub const COMMAND_DATA: u8 = 0x08;
    /// Keyboard enabled
    pub const KEYBOARD_ENABLED: u8 = 0x10;
    /// Mouse data available
    pub const MOUSE_DATA: u8 = 0x20;
    /// Timeout error
    pub const TIMEOUT_ERROR: u8 = 0x40;
    /// Parity error
    pub const PARITY_ERROR: u8 = 0x80;
}

/// PS/2 controller commands
pub mod commands {
    /// Read configuration byte
    pub const READ_CONFIG: u8 = 0x20;
    /// Write configuration byte
    pub const WRITE_CONFIG: u8 = 0x60;
    /// Disable second PS/2 port
    pub const DISABLE_SECOND_PORT: u8 = 0xA7;
    /// Enable second PS/2 port
    pub const ENABLE_SECOND_PORT: u8 = 0xA8;
    /// Test second PS/2 port
    pub const TEST_SECOND_PORT: u8 = 0xA9;
    /// Test PS/2 controller
    pub const TEST_CONTROLLER: u8 = 0xAA;
    /// Test first PS/2 port
    pub const TEST_FIRST_PORT: u8 = 0xAB;
    /// Disable first PS/2 port
    pub const DISABLE_FIRST_PORT: u8 = 0xAD;
    /// Enable first PS/2 port
    pub const ENABLE_FIRST_PORT: u8 = 0xAE;
}

/// PS/2 keyboard commands
pub mod keyboard_commands {
    /// Set LEDs
    pub const SET_LEDS: u8 = 0xED;
    /// Echo
    pub const ECHO: u8 = 0xEE;
    /// Get/set scancode set
    pub const SCANCODE_SET: u8 = 0xF0;
    /// Identify keyboard
    pub const IDENTIFY: u8 = 0xF2;
    /// Set repeat rate and delay
    pub const SET_REPEAT: u8 = 0xF3;
    /// Enable scanning
    pub const ENABLE_SCANNING: u8 = 0xF4;
    /// Disable scanning
    pub const DISABLE_SCANNING: u8 = 0xF5;
    /// Set default parameters
    pub const SET_DEFAULTS: u8 = 0xF6;
    /// Resend last byte
    pub const RESEND: u8 = 0xFE;
    /// Reset and self-test
    pub const RESET: u8 = 0xFF;
}

/// PS/2 response codes
pub mod responses {
    /// Acknowledge
    pub const ACK: u8 = 0xFA;
    /// Resend request
    pub const RESEND: u8 = 0xFE;
    /// Self-test passed
    pub const SELF_TEST_PASSED: u8 = 0xAA;
    /// Self-test failed
    pub const SELF_TEST_FAILED: u8 = 0xFC;
}

/// PS/2 controller configuration byte bits
pub mod config_bits {
    /// First PS/2 port interrupt enabled
    pub const FIRST_PORT_INTERRUPT: u8 = 0x01;
    /// Second PS/2 port interrupt enabled
    pub const SECOND_PORT_INTERRUPT: u8 = 0x02;
    /// System passed POST
    pub const SYSTEM_FLAG: u8 = 0x04;
    /// First PS/2 port clock disabled
    pub const FIRST_PORT_CLOCK_DISABLED: u8 = 0x10;
    /// Second PS/2 port clock disabled
    pub const SECOND_PORT_CLOCK_DISABLED: u8 = 0x20;
    /// First PS/2 port translation enabled
    pub const FIRST_PORT_TRANSLATION: u8 = 0x40;
}

/// Low-level PS/2 controller interface
pub struct Ps2Controller {
    data_port: Port<u8>,
    command_port: Port<u8>,
}

impl Default for Ps2Controller {
    fn default() -> Self {
        Self::new()
    }
}

impl Ps2Controller {
    /// Create a new PS/2 controller interface
    pub fn new() -> Self {
        Self {
            data_port: Port::new(PS2_DATA_PORT),
            command_port: Port::new(PS2_COMMAND_PORT),
        }
    }

    /// Read the status register
    pub fn read_status(&mut self) -> u8 {
        unsafe { self.command_port.read() }
    }

    /// Check if output buffer is full (data available to read)
    pub fn output_buffer_full(&mut self) -> bool {
        self.read_status() & status_bits::OUTPUT_BUFFER_FULL != 0
    }

    /// Check if input buffer is full (controller busy)
    pub fn input_buffer_full(&mut self) -> bool {
        self.read_status() & status_bits::INPUT_BUFFER_FULL != 0
    }

    /// Wait for the input buffer to be empty
    pub fn wait_input_buffer_empty(&mut self) {
        while self.input_buffer_full() {
            core::hint::spin_loop();
        }
    }

    /// Wait for the output buffer to be full
    pub fn wait_output_buffer_full(&mut self) {
        while !self.output_buffer_full() {
            core::hint::spin_loop();
        }
    }

    /// Read data from the PS/2 controller
    pub fn read_data(&mut self) -> u8 {
        self.wait_output_buffer_full();
        unsafe { self.data_port.read() }
    }

    /// Write data to the PS/2 controller
    pub fn write_data(&mut self, data: u8) {
        self.wait_input_buffer_empty();
        unsafe { self.data_port.write(data) }
    }

    /// Send a command to the PS/2 controller
    pub fn send_command(&mut self, command: u8) {
        self.wait_input_buffer_empty();
        unsafe { self.command_port.write(command) }
    }

    /// Send a command and read the response
    pub fn send_command_with_response(&mut self, command: u8) -> u8 {
        self.send_command(command);
        self.read_data()
    }
}

/// Initialize the PS/2 subsystem
pub fn init() -> Result<(), &'static str> {
    info!("Initializing PS/2 subsystem");
    
    let mut controller = Ps2Controller::new();
    
    controller.send_command(commands::DISABLE_FIRST_PORT);
    controller.send_command(commands::DISABLE_SECOND_PORT);
    
    while controller.output_buffer_full() {
        let _ = controller.read_data();
    }
    
    let config = controller.send_command_with_response(commands::READ_CONFIG);
    info!("PS/2 controller config: 0x{:02X}", config);
    
    let new_config = config & !(config_bits::FIRST_PORT_INTERRUPT | 
                               config_bits::SECOND_PORT_INTERRUPT | 
                               config_bits::FIRST_PORT_TRANSLATION);
    
    controller.send_command(commands::WRITE_CONFIG);
    controller.write_data(new_config);
    
    let test_result = controller.send_command_with_response(commands::TEST_CONTROLLER);
    if test_result != 0x55 {
        warn!("PS/2 controller self-test failed: 0x{:02X}", test_result);
        return Err("PS/2 controller self-test failed");
    }
    
    let port_test = controller.send_command_with_response(commands::TEST_FIRST_PORT);
    if port_test != 0x00 {
        warn!("PS/2 keyboard port test failed: 0x{:02X}", port_test);
        return Err("PS/2 keyboard port test failed");
    }
    
    controller.send_command(commands::ENABLE_FIRST_PORT);
    
    // Initialize keyboard
    keyboard::init(&mut controller)?;

    // Re-enable interrupts for the first PS/2 port (keyboard)
    let config = controller.send_command_with_response(commands::READ_CONFIG);
    let new_config = config | config_bits::FIRST_PORT_INTERRUPT;
    controller.send_command(commands::WRITE_CONFIG);
    controller.write_data(new_config);

    let final_config = controller.send_command_with_response(commands::READ_CONFIG);

    if final_config & config_bits::FIRST_PORT_INTERRUPT != 0 {
        info!("✓ PS/2 keyboard interrupts are ENABLED");
    } else {
        warn!("✗ PS/2 keyboard interrupts are DISABLED!");
    }

    info!("PS/2 subsystem initialized successfully");
    Ok(())
}