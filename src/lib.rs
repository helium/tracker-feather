//! Rust Board Support Crate (BSC) for the Helium Tracker Feather
//!
#![no_std]
extern crate stm32l0xx_hal as hal;
use hal::gpio::{Input, PullUp};
mod longfi_bindings;

pub type DebugUsart = hal::serial::USART1;
pub type GpsUsart = hal::serial::LPUART1;
pub type RadioIrq = hal::gpio::gpiob::PB0<Input<PullUp>>;

pub use longfi_bindings::get_random_bits;
pub use longfi_bindings::initialize_radio_irq;
pub use longfi_bindings::LongFiBindings;
pub use longfi_bindings::RadioIRQ;
