//! Provides abstractions for outputting text to the screen.
//!
//! This module defines the core components for writing text to the display,
//! including:
//!
//! - `console`: Manages the console display buffer and rendering.
//! - `framebuffer`: Provides a direct interface to the framebuffer.
//! - `linewriter`: Implements a simple line-based writer for the console.
//! - `flanconsole`: Provides a terminal emulator using the flanterm library.
//!
//! The main entry points are:
//!
//! - `LineWriter`: A writer that outputs to the last line of the screen,
//!   automatically shifting the buffer.
//! - `DisplayWriter`: Manages the display buffer and provides methods for
//!   writing characters and strings.
//! - `Display`: A wrapper around the framebuffer that implements the
//!   `embedded-graphics` `DrawTarget` trait.
//! - `FlanConsole`: A terminal emulator that provides ANSI escape sequence
//!   support and direct framebuffer writing.

pub mod flanconsole;
pub mod framebuffer;
pub mod macros;
pub mod tests;

pub use flanconsole::{FLANTERM, FlanConsole, flanterm_init};
