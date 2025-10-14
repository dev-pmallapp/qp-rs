#![no_std]
#![no_main]

//! ESP32-C6 Board Support Package for QP Framework
//!
//! This example demonstrates a simple blinky application using the QP framework
//! with the QV cooperative kernel on ESP32-C6.

use esp_backtrace as _;
use esp_hal::{
    clock::ClockControl,
    delay::Delay,
    gpio::{GpioPin, Output, PushPull, IO},
    peripherals::Peripherals,
    prelude::*,
    timer::timg::TimerGroup,
};
use esp_println::println;

use qp_core::{QEvent, QSignal, QStateHandler, QStateReturn};
use qp_qep::QHsm;
use qp_qf::QActive;

/// Signals for the blinky application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
enum BlinkySignal {
    Timeout = 0,
}

impl From<BlinkySignal> for QSignal {
    fn from(sig: BlinkySignal) -> Self {
        QSignal::new(sig as u16)
    }
}

/// Timeout event
struct TimeoutEvent {
    signal: QSignal,
}

impl QEvent for TimeoutEvent {
    fn signal(&self) -> QSignal {
        self.signal
    }
}

/// Blinky active object
struct BlinkyAO {
    hsm: QHsm,
    led: Option<GpioPin<Output<PushPull>, 8>>,
}

impl BlinkyAO {
    fn new() -> Self {
        Self {
            hsm: QHsm::new(),
            led: None,
        }
    }

    fn set_led(&mut self, led: GpioPin<Output<PushPull>, 8>) {
        self.led = Some(led);
    }

    // State handlers
    fn initial(_me: &mut dyn qp_core::QStateMachine, _e: &dyn QEvent) -> QStateReturn {
        println!("Blinky: Initial");
        QStateReturn::Tran(Self::on_state as QStateHandler)
    }

    fn on_state(me: &mut dyn qp_core::QStateMachine, e: &dyn QEvent) -> QStateReturn {
        if let Some(blinky) = (me as &mut dyn core::any::Any).downcast_mut::<BlinkyAO>() {
            match e.signal().value() {
                sig if sig == BlinkySignal::Timeout as u16 => {
                    println!("Blinky: ON -> OFF");
                    if let Some(led) = &mut blinky.led {
                        led.set_low();
                    }
                    QStateReturn::Tran(Self::off_state as QStateHandler)
                }
                _ => QStateReturn::Super(Self::top_state as QStateHandler),
            }
        } else {
            QStateReturn::Handled
        }
    }

    fn off_state(me: &mut dyn qp_core::QStateMachine, e: &dyn QEvent) -> QStateReturn {
        if let Some(blinky) = (me as &mut dyn core::any::Any).downcast_mut::<BlinkyAO>() {
            match e.signal().value() {
                sig if sig == BlinkySignal::Timeout as u16 => {
                    println!("Blinky: OFF -> ON");
                    if let Some(led) = &mut blinky.led {
                        led.set_high();
                    }
                    QStateReturn::Tran(Self::on_state as QStateHandler)
                }
                _ => QStateReturn::Super(Self::top_state as QStateHandler),
            }
        } else {
            QStateReturn::Handled
        }
    }

    fn top_state(_me: &mut dyn qp_core::QStateMachine, _e: &dyn QEvent) -> QStateReturn {
        QStateReturn::Handled
    }
}

impl qp_core::QStateMachine for BlinkyAO {
    fn init(&mut self, _initial_event: &dyn QEvent) {
        self.hsm.init(_initial_event);
    }

    fn dispatch(&mut self, event: &dyn QEvent) -> QStateReturn {
        self.hsm.dispatch(event)
    }

    fn current_state(&self) -> QStateHandler {
        self.hsm.current_state()
    }

    fn set_state(&mut self, state: QStateHandler) {
        self.hsm.set_state(state);
    }
}

impl QActive for BlinkyAO {
    fn priority(&self) -> qp_core::QPriority {
        qp_core::QPriority::new(1).unwrap()
    }

    fn post(&mut self, _event: &dyn QEvent) -> Result<(), ()> {
        Ok(())
    }

    fn get(&mut self) -> Option<&dyn QEvent> {
        None
    }

    fn is_empty(&self) -> bool {
        true
    }

    fn initialize(&mut self) {
        // Initialize with timeout event
        let init_event = TimeoutEvent {
            signal: BlinkySignal::Timeout.into(),
        };
        self.init(&init_event);
    }
}

#[entry]
fn main() -> ! {
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();
    
    let clocks = ClockControl::max(system.clock_control).freeze();
    let delay = Delay::new(&clocks);

    // Initialize GPIO
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);
    let led = io.pins.gpio8.into_push_pull_output();

    // Initialize watchdog timer
    let timg0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    let _timer0 = timg0.timer0;

    println!("\nQP Framework on ESP32-C6");
    println!("Blinky Example with QV Kernel\n");

    // Create blinky active object
    static mut BLINKY: BlinkyAO = BlinkyAO {
        hsm: QHsm::new(),
        led: None,
    };

    unsafe {
        BLINKY.set_led(led);
        BLINKY.initialize();
    }

    // Main loop - simple blinky without full QV integration for now
    let timeout_event = TimeoutEvent {
        signal: BlinkySignal::Timeout.into(),
    };

    loop {
        unsafe {
            let _ = BLINKY.dispatch(&timeout_event);
        }
        delay.delay_millis(500);
    }
}
