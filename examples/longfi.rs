#![no_std]
#![no_main]

extern crate nb;
extern crate panic_halt;

use hal::{exti::{self, Exti, ExtiLine as _}, prelude::*, rcc, rng::Rng, serial, syscfg};
use rtfm::app;
use stm32l0xx_hal as hal;

use longfi_device;
use longfi_device::{ClientEvent, Config, LongFi, Radio, RfEvent};

use core::fmt::Write;
use helium_tracker_feather;

static mut PRESHARED_KEY: [u8; 16] = [
    0x7B, 0x60, 0xC0, 0xF0, 0x77, 0x51, 0x50, 0xD3, 0x2, 0xCE, 0xAE, 0x50, 0xA0, 0xD2, 0x11, 0xC1,
];

#[app(device = stm32l0xx_hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        #[init([0;512])]
        buffer: [u8; 512],
        #[init(0)]
        count: u8,
        int: Exti,
        radio_irq: helium_tracker_feather::RadioIRQ,
        debug_uart: serial::Tx<helium_tracker_feather::DebugUsart>,
        uart_rx: serial::Rx<helium_tracker_feather::DebugUsart>,
        longfi: LongFi,
    }

    #[init(resources = [buffer])]
    fn init(ctx: init::Context) -> init::LateResources {
        static mut BINDINGS: Option<helium_tracker_feather::LongFiBindings> = None;
        let device = ctx.device;

        let mut rcc = device.RCC.freeze(rcc::Config::hsi16());
        let mut syscfg = syscfg::SYSCFG::new(device.SYSCFG, &mut rcc);

        let gpioa = device.GPIOA.split(&mut rcc);
        let gpiob = device.GPIOB.split(&mut rcc);
        let gpioc = device.GPIOC.split(&mut rcc);

        let (tx_pin, rx_pin, serial_peripheral) = (gpioa.pa9, gpioa.pa10, device.USART1);

        let mut serial = serial_peripheral
            .usart(
                (tx_pin, rx_pin),
                serial::Config::default().baudrate(115_200.bps()),
                &mut rcc,
            )
            .unwrap();

        // listen for incoming bytes which will trigger transmits
        serial.listen(serial::Event::Rxne);
        let (mut tx, rx) = serial.split();

        write!(tx, "LongFi Device Test\r\n").unwrap();

        let mut exti = Exti::new(device.EXTI);
        let hsi48 = rcc.enable_hsi48(&mut syscfg, device.CRS);
        let rng = Rng::new(device.RNG, &mut rcc, hsi48);
        let radio_irq =
            helium_tracker_feather::initialize_radio_irq(gpiob.pb0.into_floating_input(), &mut syscfg, &mut exti);

        *BINDINGS = Some(helium_tracker_feather::LongFiBindings::new(
            device.SPI2,
            &mut rcc,
            rng,
            gpiob.pb13,
            gpiob.pb14,
            gpiob.pb15,
            gpiob.pb12,
            gpiob.pb1,
            gpioa.pa15,
            gpioc.pc2.into_floating_input(),
            gpiob.pb4,
            gpiob.pb3,
            gpiob.pb5,
        ));

        let rf_config = Config {
            oui: 1,
            device_id: 3,
            auth_mode: longfi_device::AuthMode::PresharedKey128,
        };

        let mut longfi_radio;
        if let Some(bindings) = BINDINGS {
            longfi_radio = unsafe {
                LongFi::new(
                    Radio::sx1262(),
                    &mut bindings.bindings,
                    rf_config,
                    &PRESHARED_KEY,
                )
                .unwrap()
            };
        } else {
            panic!("No bindings exist");
        }

        longfi_radio.set_buffer(ctx.resources.buffer);

        write!(tx, "Going to main loop\r\n").unwrap();

        // Return the initialised resources.
        init::LateResources {
            int: exti,
            radio_irq,
            debug_uart: tx,
            uart_rx: rx,
            longfi: longfi_radio,
        }
    }

    #[task(capacity = 4, priority = 2, resources = [debug_uart, buffer, longfi])]
    fn radio_event(ctx: radio_event::Context, event: RfEvent) {
        let longfi_radio = ctx.resources.longfi;
        let client_event = longfi_radio.handle_event(event);

        match client_event {
            ClientEvent::ClientEvent_TxDone => {
                write!(ctx.resources.debug_uart, "Transmit Done!\r\n").unwrap();
            }
            ClientEvent::ClientEvent_Rx => {
                // get receive buffer
                let rx_packet = longfi_radio.get_rx();
                write!(ctx.resources.debug_uart, "Received packet\r\n").unwrap();
                write!(
                    ctx.resources.debug_uart,
                    "  Length =  {}\r\n",
                    rx_packet.len
                )
                .unwrap();
                write!(
                    ctx.resources.debug_uart,
                    "  Rssi   = {}\r\n",
                    rx_packet.rssi
                )
                .unwrap();
                write!(
                    ctx.resources.debug_uart,
                    "  Snr    =  {}\r\n",
                    rx_packet.snr
                )
                .unwrap();
                unsafe {
                    for i in 0..rx_packet.len {
                        write!(
                            ctx.resources.debug_uart,
                            "{:X} ",
                            *rx_packet.buf.offset(i as isize)
                        )
                        .unwrap();
                    }
                    write!(ctx.resources.debug_uart, "\r\n").unwrap();
                }
                // give buffer back to library
                longfi_radio.set_buffer(ctx.resources.buffer);
            }
            ClientEvent::ClientEvent_None => {}
        }
    }

    #[task(capacity = 4, priority = 2, resources = [debug_uart, count, longfi])]
    fn send_ping(ctx: send_ping::Context) {
        let longfi = ctx.resources.longfi;
        let tx = ctx.resources.debug_uart;

        write!(tx, "Sending Ping\r\n").unwrap();
        let packet: [u8; 14] = [
            1,
            2,
            3,
            4,
            *ctx.resources.count,
            5,
            6,
            7,
            8,
            9,
            10,
            12,
            13,
            14,
        ];
        *ctx.resources.count += 1;
        longfi.send(&packet);
    }

    #[task(binds = USART1, priority=1, resources = [uart_rx], spawn = [send_ping])]
    fn USART1(ctx: USART1::Context) {
        let rx = ctx.resources.uart_rx;
        rx.read().unwrap();
        ctx.spawn.send_ping().unwrap();
    }

    #[task(binds = EXTI0_1, priority = 1, resources = [radio_irq, int], spawn = [radio_event])]
    fn EXTI0_1(ctx: EXTI0_1::Context) {
        Exti::unpend(exti::GpioLine::from_raw_line(ctx.resources.radio_irq.pin_number()).unwrap());
        ctx.spawn.radio_event(RfEvent::DIO0).unwrap();
    }

    // Interrupt handlers used to dispatch software tasks
    extern "C" {
        fn USART4_USART5();
    }
};
