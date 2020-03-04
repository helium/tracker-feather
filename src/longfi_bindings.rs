use crate::hal::prelude::*;
use hal::exti::{self, Exti, ExtiLine as _};
use hal::gpio::*;
use hal::pac;
use hal::rcc::Rcc;
use hal::rng;
use hal::spi;
use longfi_device::{AntPinsMode, BoardBindings};
use nb::block;

#[allow(dead_code)]
pub struct LongFiBindings {
    pub bindings: BoardBindings,
}

type Uninitialized = Analog;

pub type RadioIRQ = gpiob::PB0<Input<Floating>>;

pub fn initialize_radio_irq(
    pin: gpiob::PB0<Input<Floating>>,
    syscfg: &mut hal::syscfg::SYSCFG,
    exti: &mut Exti,
) -> RadioIRQ {
    exti.listen_gpio(
        syscfg,
        pin.port(),
        exti::GpioLine::from_raw_line(pin.pin_number()).unwrap(),
        exti::TriggerEdge::Rising,
    );

    pin
}

impl LongFiBindings {
    pub fn new(
        spi_peripheral: pac::SPI2,
        rcc: &mut Rcc,
        rng: rng::Rng,
        spi_sck: gpiob::PB13<Uninitialized>,
        spi_miso: gpiob::PB14<Uninitialized>,
        spi_mosi: gpiob::PB15<Uninitialized>,
        spi_nss_pin: gpiob::PB12<Uninitialized>,
        reset: gpiob::PB1<Uninitialized>,
        ant_en: gpioa::PA15<Uninitialized>,
        radio_busy: gpioc::PC2<Input<Floating>>,
        se_csd: gpiob::PB4<Uninitialized>,
        se_cps: gpiob::PB3<Uninitialized>,
        se_ctx: gpiob::PB5<Uninitialized>,
    ) -> LongFiBindings {
        // store all of the necessary pins and peripherals into statics
        // this is necessary as the extern C functions need access
        // this is safe, thanks to ownership and because these statics are private
        unsafe {
            SPI = Some(spi_peripheral.spi(
                (spi_sck, spi_miso, spi_mosi),
                spi::MODE_0,
                1_000_000.hz(),
                rcc,
            ));
            SPI_NSS = Some(spi_nss_pin.into_push_pull_output());
            RESET = Some(reset.into_push_pull_output());
            RADIO_BUSY = Some(radio_busy);
            RNG = Some(rng);
            ANT_SW = Some(AntSw {
                ant_en: ant_en.into_push_pull_output(),
                se_csd: se_csd.into_push_pull_output(),
                se_cps: se_cps.into_push_pull_output(),
                se_ctx: se_ctx.into_push_pull_output(),
            })
        };

        LongFiBindings {
            bindings: BoardBindings {
                reset: Some(radio_reset),
                spi_in_out: Some(spi_in_out),
                spi_nss: Some(spi_nss),
                delay_ms: Some(delay_ms),
                get_random_bits: Some(get_random_bits),
                set_antenna_pins: Some(set_antenna_pins),
                set_board_tcxo: None,
                busy_pin_status: Some(busy_pin_status),
                reduce_power: Some(reduce_power),
            },
        }
    }
}

pub struct AntennaSwitches<AntEn, SeCsd, SeCps, SeCtx> {
    ant_en: AntEn,
    se_csd: SeCsd,
    se_cps: SeCps,
    se_ctx: SeCtx,
}

extern "C" fn reduce_power(power: u8) -> u8 {
    17
}

impl<AntEn, SeCsd, SeCps, SeCtx> AntennaSwitches<AntEn, SeCsd, SeCps, SeCtx>
where
    AntEn: embedded_hal::digital::v2::OutputPin,
    SeCsd: embedded_hal::digital::v2::OutputPin,
    SeCps: embedded_hal::digital::v2::OutputPin,
    SeCtx: embedded_hal::digital::v2::OutputPin,
{
    pub fn new(
        ant_en: AntEn,
        se_csd: SeCsd,
        se_cps: SeCps,
        se_ctx: SeCtx,
    ) -> AntennaSwitches<AntEn, SeCsd, SeCps, SeCtx> {
        AntennaSwitches {
            ant_en,
            se_csd,
            se_cps,
            se_ctx,
        }
    }

    pub fn set_sleep(&mut self) {
        self.ant_en.set_low();
        self.se_cps.set_low();
        self.se_csd.set_low();
        self.se_ctx.set_low();
    }

    pub fn set_tx(&mut self) {
        self.ant_en.set_low();
        self.se_cps.set_high();
        self.se_csd.set_high();
        self.se_ctx.set_high();
    }

    pub fn set_rx(&mut self) {
        self.ant_en.set_high();
        self.se_cps.set_high();
        self.se_csd.set_high();
        self.se_ctx.set_low();
    }
}

type AntSw = AntennaSwitches<
    gpioa::PA15<Output<PushPull>>,
    gpiob::PB4<Output<PushPull>>,
    gpiob::PB3<Output<PushPull>>,
    gpiob::PB5<Output<PushPull>>,
>;

static mut ANT_SW: Option<AntSw> = None;

extern "C" fn set_antenna_pins(mode: AntPinsMode, _power: u8) {
    unsafe {
        if let Some(ant_sw) = &mut ANT_SW {
            match mode {
                AntPinsMode::AntModeTx => {
                    ant_sw.set_tx();
                }
                AntPinsMode::AntModeRx => {
                    ant_sw.set_rx();
                }
                AntPinsMode::AntModeSleep => {
                    ant_sw.set_sleep();
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
        hal::serial::PB13<Uninitialized>,
        hal::serial::PB14<Uninitialized>,
        hal::serial::PB15<Uninitialized>,
    ),
>;
static mut SPI: Option<SpiPort> = None;
#[no_mangle]
extern "C" fn spi_in_out(out_data: u8) -> u8 {
    unsafe {
        if let Some(spi) = &mut SPI {
            spi.send(out_data).unwrap();
            let in_data = block!(spi.read()).unwrap();
            in_data
        } else {
            panic!("No SPI");
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
        } else {
            panic!("No SPI_NSS");
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
        } else {
            panic!("No Radio Reset");
        }
    }
}

#[no_mangle]
extern "C" fn delay_ms(ms: u32) {
    cortex_m::asm::delay(ms);
}

static mut RNG: Option<rng::Rng> = None;
pub extern "C" fn get_random_bits(_bits: u8) -> u32 {
    unsafe {
        if let Some(rng) = &mut RNG {
            // enable starts the ADC conversions that generate the random number
            rng.enable();
            // wait until the flag flips; interrupt driven is possible but no implemented
            rng.wait();
            // reading the result clears the ready flag
            let val = rng.take_result();
            // can save some power by disabling until next random number needed
            rng.disable();
            val
        } else {
            panic!("No Rng exists!");
        }
    }
}
