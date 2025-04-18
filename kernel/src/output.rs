//! Provides abstractions for outputting text to the screen.
//!
//! This module defines the core components for writing text to the display,
//! including:
//!
//! - `console`: Manages the console display buffer and rendering.
//! - `framebuffer`: Provides a direct interface to the framebuffer.
//! - `linewriter`: Implements a simple line-based writer for the console.
//!
//! The main entry points are:
//!
//! - `LineWriter`: A writer that outputs to the last line of the screen,
//!   automatically shifting the buffer.
//! - `DisplayWriter`: Manages the display buffer and provides methods for
//!   writing characters and strings.
//! - `Display`: A wrapper around the framebuffer that implements the
//!   `embedded-graphics` `DrawTarget` trait.

pub mod console;
pub mod framebuffer;
pub mod linewriter;

pub use console::DisplayWriter;
pub use framebuffer::Display;
pub use linewriter::LineWriter;
