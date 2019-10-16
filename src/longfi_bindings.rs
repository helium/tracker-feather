use crate::hal::prelude::InputPin;
use crate::hal::prelude::OutputPin;
use crate::hal::prelude::*;
use hal::device;
use hal::gpio::*;
use hal::rcc::Rcc;
use hal::spi;
use longfi_device::{AntPinsMode, BoardBindings, Spi};
use nb::block;

pub fn new(
    spi_peripheral: device::SPI2,
    rcc: &mut Rcc,
    gpioa: gpioa::Parts,
    gpiob: gpiob::Parts,
    gpioc: gpioc::Parts,
) -> BoardBindings {
    // store all of the necessary pins and peripherals into statics
    // this is necessary as the extern C functions need access
    // this is safe, thanks to ownership and because these statics are private
    unsafe {
        ANT_EN = Some(gpioa.pa15.into_push_pull_output());
        RADIO_BUSY = Some(gpioc.pc2.into_floating_input());
        SPI = Some(spi_peripheral.spi(
            (gpiob.pb13, gpiob.pb14, gpiob.pb15),
            spi::MODE_0,
            1_000_000.hz(),
            rcc,
        ));
        SPI_NSS = Some(gpiob.pb12.into_push_pull_output());
        RESET = Some(gpiob.pb1.into_push_pull_output());
    };

    BoardBindings {
        reset: Some(radio_reset),
        spi_in_out: Some(spi_in_out),
        spi_nss: Some(spi_nss),
        delay_ms: Some(delay_ms),
        get_random_bits: Some(get_random_bits),
        set_antenna_pins: Some(set_antenna_pins),
        set_board_tcxo: None,
        busy_pin_status: Some(busy_pin_status),
        reduce_power: None,
    }
}

static mut ANT_EN: Option<gpioa::PA15<Output<PushPull>>> = None;
extern "C" fn set_antenna_pins(mode: AntPinsMode, _power: u8) {
    unsafe {
        if let Some(ant_en) = &mut ANT_EN {
            match mode {
                AntPinsMode::AntModeTx | AntPinsMode::AntModeRx => {
                    ant_en.set_high().unwrap();
                }
                AntPinsMode::AntModeSleep => {
                    ant_en.set_low().unwrap();
                }
                _ => (),
            }
        }
    }
}

static mut RADIO_BUSY: Option<gpioc::PC2<Input<Floating>>> = None;
#[no_mangle]
extern "C" fn busy_pin_status() -> bool {
    unsafe {
        if let Some(pin) = &mut RADIO_BUSY {
            return pin.is_high().unwrap();
        }
        true
    }
}

type SpiPort = hal::spi::Spi<
    hal::pac::SPI2,
    (
        hal::serial::PB13<hal::gpio::Input<hal::gpio::Floating>>,
        hal::serial::PB14<hal::gpio::Input<hal::gpio::Floating>>,
        hal::serial::PB15<hal::gpio::Input<hal::gpio::Floating>>,
    ),
>;
static mut SPI: Option<SpiPort> = None;
#[no_mangle]
extern "C" fn spi_in_out(_s: *mut Spi, out_data: u8) -> u8 {
    unsafe {
        if let Some(spi) = &mut SPI {
            spi.send(out_data).unwrap();
            let in_data = block!(spi.read()).unwrap();
            in_data
        } else {
            0
        }
    }
}

static mut SPI_NSS: Option<gpiob::PB12<Output<PushPull>>> = None;
#[no_mangle]
extern "C" fn spi_nss(value: bool) {
    unsafe {
        if let Some(pin) = &mut SPI_NSS {
            if value {
                pin.set_high().unwrap();
            } else {
                pin.set_low().unwrap();
            }
        }
    }
}

static mut RESET: Option<gpiob::PB1<Output<PushPull>>> = None;
#[no_mangle]
extern "C" fn radio_reset(value: bool) {
    unsafe {
        if let Some(pin) = &mut RESET {
            if value {
                pin.set_low().unwrap();
            } else {
                pin.set_high().unwrap();
            }
        }
    }
}

#[no_mangle]
extern "C" fn delay_ms(ms: u32) {
    cortex_m::asm::delay(ms);
}

#[no_mangle]
extern "C" fn get_random_bits(_bits: u8) -> u32 {
    0x1
}
