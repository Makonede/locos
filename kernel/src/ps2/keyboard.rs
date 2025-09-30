//! PS/2 keyboard driver implementation.
//!
//! This module handles PS/2 keyboard initialization, interrupt handling,
//! and provides an interface for reading keyboard input.

use crate::{info, warn, debug};
use alloc::collections::VecDeque;
use spin::Mutex;
use x86_64::instructions::port::Port;

use super::{Ps2Controller, keyboard_commands, responses};

/// Maximum size of the keyboard input buffer
const KEYBOARD_BUFFER_SIZE: usize = 256;

/// Keyboard scan codes (Set 1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ScanCode {
    // Letters
    A = 0x1E, B = 0x30, C = 0x2E, D = 0x20, E = 0x12, F = 0x21, G = 0x22,
    H = 0x23, I = 0x17, J = 0x24, K = 0x25, L = 0x26, M = 0x32, N = 0x31,
    O = 0x18, P = 0x19, Q = 0x10, R = 0x13, S = 0x1F, T = 0x14, U = 0x16,
    V = 0x2F, W = 0x11, X = 0x2D, Y = 0x15, Z = 0x2C,
    
    // Numbers
    Key1 = 0x02, Key2 = 0x03, Key3 = 0x04, Key4 = 0x05, Key5 = 0x06,
    Key6 = 0x07, Key7 = 0x08, Key8 = 0x09, Key9 = 0x0A, Key0 = 0x0B,
    
    // Function keys
    F1 = 0x3B, F2 = 0x3C, F3 = 0x3D, F4 = 0x3E, F5 = 0x3F, F6 = 0x40,
    F7 = 0x41, F8 = 0x42, F9 = 0x43, F10 = 0x44, F11 = 0x57, F12 = 0x58,
    
    // Special keys
    Escape = 0x01,
    Backspace = 0x0E,
    Tab = 0x0F,
    Enter = 0x1C,
    Space = 0x39,
    
    // Modifier keys
    LeftShift = 0x2A,
    RightShift = 0x36,
    LeftCtrl = 0x1D,
    LeftAlt = 0x38,
    CapsLock = 0x3A,
    
    // Punctuation
    Minus = 0x0C,
    Equals = 0x0D,
    LeftBracket = 0x1A,
    RightBracket = 0x1B,
    Semicolon = 0x27,
    Quote = 0x28,
    Grave = 0x29,
    Backslash = 0x2B,
    Comma = 0x33,
    Period = 0x34,
    Slash = 0x35,
    
    // Arrow keys (extended)
    UpArrow = 0x48,
    DownArrow = 0x50,
    LeftArrow = 0x4B,
    RightArrow = 0x4D,
    
    // Other
    Delete = 0x53,
    Home = 0x47,
    End = 0x4F,
    PageUp = 0x49,
    PageDown = 0x51,
    Insert = 0x52,
}

impl ScanCode {
    /// Check if this scancode represents a printable character
    pub fn is_character(&self) -> bool {
        matches!(self,
            ScanCode::A | ScanCode::B | ScanCode::C | ScanCode::D | ScanCode::E |
            ScanCode::F | ScanCode::G | ScanCode::H | ScanCode::I | ScanCode::J |
            ScanCode::K | ScanCode::L | ScanCode::M | ScanCode::N | ScanCode::O |
            ScanCode::P | ScanCode::Q | ScanCode::R | ScanCode::S | ScanCode::T |
            ScanCode::U | ScanCode::V | ScanCode::W | ScanCode::X | ScanCode::Y |
            ScanCode::Z |
            ScanCode::Key1 | ScanCode::Key2 | ScanCode::Key3 | ScanCode::Key4 |
            ScanCode::Key5 | ScanCode::Key6 | ScanCode::Key7 | ScanCode::Key8 |
            ScanCode::Key9 | ScanCode::Key0 |
            ScanCode::Minus | ScanCode::Equals | ScanCode::LeftBracket | ScanCode::RightBracket |
            ScanCode::Semicolon | ScanCode::Quote | ScanCode::Grave | ScanCode::Backslash |
            ScanCode::Comma | ScanCode::Period | ScanCode::Slash | ScanCode::Space
        )
    }

    /// Convert scancode to character, considering shift state
    /// Returns None if the scancode doesn't represent a printable character
    pub fn to_char(&self, shift_pressed: bool, caps_lock: bool) -> Option<char> {
        match self {
            ScanCode::A => Some(if shift_pressed ^ caps_lock { 'A' } else { 'a' }),
            ScanCode::B => Some(if shift_pressed ^ caps_lock { 'B' } else { 'b' }),
            ScanCode::C => Some(if shift_pressed ^ caps_lock { 'C' } else { 'c' }),
            ScanCode::D => Some(if shift_pressed ^ caps_lock { 'D' } else { 'd' }),
            ScanCode::E => Some(if shift_pressed ^ caps_lock { 'E' } else { 'e' }),
            ScanCode::F => Some(if shift_pressed ^ caps_lock { 'F' } else { 'f' }),
            ScanCode::G => Some(if shift_pressed ^ caps_lock { 'G' } else { 'g' }),
            ScanCode::H => Some(if shift_pressed ^ caps_lock { 'H' } else { 'h' }),
            ScanCode::I => Some(if shift_pressed ^ caps_lock { 'I' } else { 'i' }),
            ScanCode::J => Some(if shift_pressed ^ caps_lock { 'J' } else { 'j' }),
            ScanCode::K => Some(if shift_pressed ^ caps_lock { 'K' } else { 'k' }),
            ScanCode::L => Some(if shift_pressed ^ caps_lock { 'L' } else { 'l' }),
            ScanCode::M => Some(if shift_pressed ^ caps_lock { 'M' } else { 'm' }),
            ScanCode::N => Some(if shift_pressed ^ caps_lock { 'N' } else { 'n' }),
            ScanCode::O => Some(if shift_pressed ^ caps_lock { 'O' } else { 'o' }),
            ScanCode::P => Some(if shift_pressed ^ caps_lock { 'P' } else { 'p' }),
            ScanCode::Q => Some(if shift_pressed ^ caps_lock { 'Q' } else { 'q' }),
            ScanCode::R => Some(if shift_pressed ^ caps_lock { 'R' } else { 'r' }),
            ScanCode::S => Some(if shift_pressed ^ caps_lock { 'S' } else { 's' }),
            ScanCode::T => Some(if shift_pressed ^ caps_lock { 'T' } else { 't' }),
            ScanCode::U => Some(if shift_pressed ^ caps_lock { 'U' } else { 'u' }),
            ScanCode::V => Some(if shift_pressed ^ caps_lock { 'V' } else { 'v' }),
            ScanCode::W => Some(if shift_pressed ^ caps_lock { 'W' } else { 'w' }),
            ScanCode::X => Some(if shift_pressed ^ caps_lock { 'X' } else { 'x' }),
            ScanCode::Y => Some(if shift_pressed ^ caps_lock { 'Y' } else { 'y' }),
            ScanCode::Z => Some(if shift_pressed ^ caps_lock { 'Z' } else { 'z' }),

            ScanCode::Key1 => Some(if shift_pressed { '!' } else { '1' }),
            ScanCode::Key2 => Some(if shift_pressed { '@' } else { '2' }),
            ScanCode::Key3 => Some(if shift_pressed { '#' } else { '3' }),
            ScanCode::Key4 => Some(if shift_pressed { '$' } else { '4' }),
            ScanCode::Key5 => Some(if shift_pressed { '%' } else { '5' }),
            ScanCode::Key6 => Some(if shift_pressed { '^' } else { '6' }),
            ScanCode::Key7 => Some(if shift_pressed { '&' } else { '7' }),
            ScanCode::Key8 => Some(if shift_pressed { '*' } else { '8' }),
            ScanCode::Key9 => Some(if shift_pressed { '(' } else { '9' }),
            ScanCode::Key0 => Some(if shift_pressed { ')' } else { '0' }),

            ScanCode::Minus => Some(if shift_pressed { '_' } else { '-' }),
            ScanCode::Equals => Some(if shift_pressed { '+' } else { '=' }),
            ScanCode::LeftBracket => Some(if shift_pressed { '{' } else { '[' }),
            ScanCode::RightBracket => Some(if shift_pressed { '}' } else { ']' }),
            ScanCode::Semicolon => Some(if shift_pressed { ':' } else { ';' }),
            ScanCode::Quote => Some(if shift_pressed { '"' } else { '\'' }),
            ScanCode::Grave => Some(if shift_pressed { '~' } else { '`' }),
            ScanCode::Backslash => Some(if shift_pressed { '|' } else { '\\' }),
            ScanCode::Comma => Some(if shift_pressed { '<' } else { ',' }),
            ScanCode::Period => Some(if shift_pressed { '>' } else { '.' }),
            ScanCode::Slash => Some(if shift_pressed { '?' } else { '/' }),

            ScanCode::Space => Some(' '),
            ScanCode::Enter => Some('\n'),

            _ => None,
        }
    }
}

/// Key event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// Key was pressed
    KeyDown(ScanCode),
    /// Key was released
    KeyUp(ScanCode),
    /// Unknown scancode received
    Unknown(u8),
}

/// Keyboard state tracking modifier keys
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardState {
    pub left_shift: bool,
    pub right_shift: bool,
    pub left_ctrl: bool,
    pub left_alt: bool,
    pub caps_lock: bool,
}

impl KeyboardState {
    /// Check if any shift key is pressed
    pub fn shift_pressed(&self) -> bool {
        self.left_shift || self.right_shift
    }
    
    /// Update state based on key event
    pub fn update(&mut self, event: KeyEvent) {
        match event {
            KeyEvent::KeyDown(scancode) => {
                match scancode {
                    ScanCode::LeftShift => self.left_shift = true,
                    ScanCode::RightShift => self.right_shift = true,
                    ScanCode::LeftCtrl => self.left_ctrl = true,
                    ScanCode::LeftAlt => self.left_alt = true,
                    ScanCode::CapsLock => self.caps_lock = !self.caps_lock,
                    _ => {}
                }
            }
            KeyEvent::KeyUp(scancode) => {
                match scancode {
                    ScanCode::LeftShift => self.left_shift = false,
                    ScanCode::RightShift => self.right_shift = false,
                    ScanCode::LeftCtrl => self.left_ctrl = false,
                    ScanCode::LeftAlt => self.left_alt = false,
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// Global keyboard state and input buffer
pub static KEYBOARD: Mutex<Option<KeyboardDriver>> = Mutex::new(None);

/// Keyboard driver state
pub struct KeyboardDriver {
    input_buffer: VecDeque<KeyEvent>,
    state: KeyboardState,
    extended_scancode: bool,
}

impl KeyboardDriver {
    /// Create a new keyboard driver
    fn new() -> Self {
        Self {
            input_buffer: VecDeque::with_capacity(KEYBOARD_BUFFER_SIZE),
            state: KeyboardState::default(),
            extended_scancode: false,
        }
    }
    
    /// Process a raw scancode from the keyboard
    pub fn process_scancode(&mut self, scancode: u8) {
        if scancode == 0xE0 {
            self.extended_scancode = true;
            return;
        }
        let is_release = scancode & 0x80 != 0;
        let base_scancode = scancode & 0x7F;
        
        let scan_code = match self.scancode_to_enum(base_scancode, self.extended_scancode) {
            Some(sc) => sc,
            None => {
                debug!("Unknown scancode: 0x{:02X} (extended: {})", base_scancode, self.extended_scancode);
                self.extended_scancode = false;
                let event = KeyEvent::Unknown(scancode);
                if self.input_buffer.len() < KEYBOARD_BUFFER_SIZE {
                    self.input_buffer.push_back(event);
                }
                return;
            }
        };
        
        let event = if is_release {
            KeyEvent::KeyUp(scan_code)
        } else {
            KeyEvent::KeyDown(scan_code)
        };
        
        self.state.update(event);
        
        if self.input_buffer.len() < KEYBOARD_BUFFER_SIZE {
            self.input_buffer.push_back(event);
        } else {
            warn!("Keyboard input buffer overflow");
        }
        
        self.extended_scancode = false;
    }
    
    /// Convert raw scancode to ScanCode enum
    fn scancode_to_enum(&self, scancode: u8, extended: bool) -> Option<ScanCode> {
        if extended {
            match scancode {
                0x48 => Some(ScanCode::UpArrow),
                0x50 => Some(ScanCode::DownArrow),
                0x4B => Some(ScanCode::LeftArrow),
                0x4D => Some(ScanCode::RightArrow),
                0x53 => Some(ScanCode::Delete),
                0x47 => Some(ScanCode::Home),
                0x4F => Some(ScanCode::End),
                0x49 => Some(ScanCode::PageUp),
                0x51 => Some(ScanCode::PageDown),
                0x52 => Some(ScanCode::Insert),
                _ => None,
            }
        } else {
            match scancode {
                0x1E => Some(ScanCode::A), 0x30 => Some(ScanCode::B), 0x2E => Some(ScanCode::C),
                0x20 => Some(ScanCode::D), 0x12 => Some(ScanCode::E), 0x21 => Some(ScanCode::F),
                0x22 => Some(ScanCode::G), 0x23 => Some(ScanCode::H), 0x17 => Some(ScanCode::I),
                0x24 => Some(ScanCode::J), 0x25 => Some(ScanCode::K), 0x26 => Some(ScanCode::L),
                0x32 => Some(ScanCode::M), 0x31 => Some(ScanCode::N), 0x18 => Some(ScanCode::O),
                0x19 => Some(ScanCode::P), 0x10 => Some(ScanCode::Q), 0x13 => Some(ScanCode::R),
                0x1F => Some(ScanCode::S), 0x14 => Some(ScanCode::T), 0x16 => Some(ScanCode::U),
                0x2F => Some(ScanCode::V), 0x11 => Some(ScanCode::W), 0x2D => Some(ScanCode::X),
                0x15 => Some(ScanCode::Y), 0x2C => Some(ScanCode::Z),
                
                0x02 => Some(ScanCode::Key1), 0x03 => Some(ScanCode::Key2), 0x04 => Some(ScanCode::Key3),
                0x05 => Some(ScanCode::Key4), 0x06 => Some(ScanCode::Key5), 0x07 => Some(ScanCode::Key6),
                0x08 => Some(ScanCode::Key7), 0x09 => Some(ScanCode::Key8), 0x0A => Some(ScanCode::Key9),
                0x0B => Some(ScanCode::Key0),
                
                0x3B => Some(ScanCode::F1), 0x3C => Some(ScanCode::F2), 0x3D => Some(ScanCode::F3),
                0x3E => Some(ScanCode::F4), 0x3F => Some(ScanCode::F5), 0x40 => Some(ScanCode::F6),
                0x41 => Some(ScanCode::F7), 0x42 => Some(ScanCode::F8), 0x43 => Some(ScanCode::F9),
                0x44 => Some(ScanCode::F10), 0x57 => Some(ScanCode::F11), 0x58 => Some(ScanCode::F12),
                
                0x01 => Some(ScanCode::Escape), 0x0E => Some(ScanCode::Backspace), 0x0F => Some(ScanCode::Tab),
                0x1C => Some(ScanCode::Enter), 0x39 => Some(ScanCode::Space),
                
                0x2A => Some(ScanCode::LeftShift), 0x36 => Some(ScanCode::RightShift),
                0x1D => Some(ScanCode::LeftCtrl), 0x38 => Some(ScanCode::LeftAlt), 0x3A => Some(ScanCode::CapsLock),
                
                0x0C => Some(ScanCode::Minus), 0x0D => Some(ScanCode::Equals), 0x1A => Some(ScanCode::LeftBracket),
                0x1B => Some(ScanCode::RightBracket), 0x27 => Some(ScanCode::Semicolon), 0x28 => Some(ScanCode::Quote),
                0x29 => Some(ScanCode::Grave), 0x2B => Some(ScanCode::Backslash), 0x33 => Some(ScanCode::Comma),
                0x34 => Some(ScanCode::Period), 0x35 => Some(ScanCode::Slash),
                
                _ => None,
            }
        }
    }
    
    /// Read the next key event from the buffer
    pub fn read_key(&mut self) -> Option<KeyEvent> {
        self.input_buffer.pop_front()
    }
    
    /// Get the current keyboard state
    pub fn get_state(&self) -> KeyboardState {
        self.state
    }
    
    /// Check if there are pending key events
    pub fn has_key(&self) -> bool {
        !self.input_buffer.is_empty()
    }
}

/// Initialize the keyboard
pub fn init(controller: &mut Ps2Controller) -> Result<(), &'static str> {
    info!("Initializing PS/2 keyboard");
    
    controller.write_data(keyboard_commands::RESET);
    let response = controller.read_data();
    if response != responses::ACK {
        warn!("Keyboard reset failed to ACK: 0x{:02X}", response);
        return Err("Keyboard reset failed");
    }
    
    let self_test = controller.read_data();
    if self_test != responses::SELF_TEST_PASSED {
        warn!("Keyboard self-test failed: 0x{:02X}", self_test);
        return Err("Keyboard self-test failed");
    }
    
    controller.write_data(keyboard_commands::SCANCODE_SET);
    let ack = controller.read_data();
    if ack != responses::ACK {
        warn!("Scancode set command failed: 0x{:02X}", ack);
        return Err("Scancode set command failed");
    }
    
    controller.write_data(0x01); // Set 1
    let ack2 = controller.read_data();
    if ack2 != responses::ACK {
        warn!("Scancode set 1 selection failed: 0x{:02X}", ack2);
        return Err("Scancode set selection failed");
    }
    
    controller.write_data(keyboard_commands::ENABLE_SCANNING);
    let ack3 = controller.read_data();
    if ack3 != responses::ACK {
        warn!("Enable scanning failed: 0x{:02X}", ack3);
        return Err("Enable scanning failed");
    }
    
    let mut keyboard_lock = KEYBOARD.lock();
    *keyboard_lock = Some(KeyboardDriver::new());
    
    info!("PS/2 keyboard initialized successfully");
    Ok(())
}

/// Handle keyboard interrupt (called from interrupt handler)
#[inline(always)]
pub fn handle_interrupt() {
    let mut data_port = Port::<u8>::new(0x60);
    let mut status_port = Port::<u8>::new(0x64);

    while unsafe { status_port.read() } & 0x01 != 0 { // While output buffer full
        let scancode = unsafe { data_port.read() };

        let mut keyboard_lock = KEYBOARD.lock();
        if let Some(ref mut keyboard) = *keyboard_lock {
            keyboard.process_scancode(scancode);
        }
    }
}

/// Read the next key event
pub fn read_key() -> Option<KeyEvent> {
    let mut keyboard_lock = KEYBOARD.lock();
    if let Some(ref mut keyboard) = *keyboard_lock {
        keyboard.read_key()
    } else {
        None
    }
}

/// Check if there are pending key events
pub fn has_key() -> bool {
    let keyboard_lock = KEYBOARD.lock();
    if let Some(ref keyboard) = *keyboard_lock {
        keyboard.has_key()
    } else {
        false
    }
}

/// Get current keyboard state (public API)
pub fn get_keyboard_state() -> Option<KeyboardState> {
    let keyboard_lock = KEYBOARD.lock();
    (*keyboard_lock).as_ref().map(|keyboard| keyboard.get_state())
}

/// Convert a key event to a character if possible
/// Returns None for non-character keys or key releases
pub fn key_event_to_char(event: KeyEvent) -> Option<char> {
    match event {
        KeyEvent::KeyDown(scancode) => {
            if let Some(state) = get_keyboard_state() {
                scancode.to_char(state.shift_pressed(), state.caps_lock)
            } else {
                // Fallback if keyboard state is not available
                scancode.to_char(false, false)
            }
        }
        _ => None, // Key releases and unknown keys don't produce characters
    }
}

/// Check if a key event represents a printable character
pub fn is_character_key(event: KeyEvent) -> bool {
    match event {
        KeyEvent::KeyDown(scancode) => scancode.is_character(),
        _ => false,
    }
}
