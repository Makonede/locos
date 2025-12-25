//! Interrupt handling for locOS.
//!
//! This module provides interrupt handling infrastructure including:
//! - Interrupt Descriptor Table (IDT) setup
//! - APIC (Advanced Programmable Interrupt Controller) support
//! - PIC (Programmable Interrupt Controller) legacy support

pub mod apic;
pub mod idt;
pub mod pic;

pub use apic::setup_apic;
pub use idt::init_idt;
