#![cfg(feature = "rt")]

use alloc::sync::Arc;

use qf::time::TimeEvent;
use qk::{QkKernel, QkKernelBuilder, QkKernelError, QkTimeEventError, QkTimerWheel};

use crate::{Esp32C6Port, PortConfig};

/// QK runtime harness for the ESP32-C6 port.
pub struct Esp32C6QkRuntime {
    kernel: Arc<QkKernel>,
    timers: QkTimerWheel,
    port: Esp32C6Port,
    config: PortConfig,
}

impl Esp32C6QkRuntime {
    /// Wraps an already constructed kernel and initialises the port.
    pub fn new(kernel: Arc<QkKernel>, mut port: Esp32C6Port, config: PortConfig) -> Self {
        port.init_interrupts();
        port.init_system_timer(config.tick_hz);

        let timers = QkTimerWheel::new(Arc::clone(&kernel));

        Self {
            kernel,
            timers,
            port,
            config,
        }
    }

    /// Builds a kernel from the provided builder and starts it before
    /// initialising the port.
    pub fn with_builder(
        builder: QkKernelBuilder,
        port: Esp32C6Port,
        config: PortConfig,
    ) -> Result<Self, QkKernelError> {
        let kernel = Arc::new(builder.build()?);
        kernel.start();
        Ok(Self::new(kernel, port, config))
    }

    /// Returns a clone of the kernel handle stored in this runtime.
    pub fn kernel(&self) -> Arc<QkKernel> {
        Arc::clone(&self.kernel)
    }

    /// Gives access to the embedded port instance.
    pub fn port(&self) -> &Esp32C6Port {
        &self.port
    }

    /// Mutable access to the port for late hardware configuration.
    pub fn port_mut(&mut self) -> &mut Esp32C6Port {
        &mut self.port
    }

    /// Retrieves the configuration used to start the runtime.
    pub fn config(&self) -> PortConfig {
        self.config
    }

    /// Registers a time event with the timer wheel.
    pub fn register_time_event(&mut self, event: Arc<TimeEvent>) {
        self.timers.register(event);
    }

    /// Processes a single tick from the underlying hardware timer.
    pub fn tick(&self) -> Result<(), QkTimeEventError> {
        self.timers.tick()
    }

    /// Runs the kernel until all ready work completes.
    pub fn run_until_idle(&self) {
        self.kernel.run_until_idle();
    }

    /// Indicates whether there is outstanding work for the kernel.
    pub fn has_pending_work(&self) -> bool {
        self.kernel.has_pending_work()
    }
}

#[cfg(all(test, feature = "rt"))]
mod tests {
    use super::*;
    use alloc::sync::Arc;

    use qf::active::{new_active_object, ActiveContext, ActiveObjectId, SignalHandler};
    use qf::event::Signal;
    use qf::time::TimeEventConfig;

    #[derive(Clone)]
    struct Recorder {
        id: ActiveObjectId,
        counter: Arc<core::sync::atomic::AtomicUsize>,
    }

    impl Recorder {
        fn new(id: ActiveObjectId, counter: Arc<core::sync::atomic::AtomicUsize>) -> Self {
            Self { id, counter }
        }
    }

    impl SignalHandler for Recorder {
        fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActiveContext) {
            if signal == Signal(42) {
                self.counter
                    .fetch_add(1, core::sync::atomic::Ordering::SeqCst);
            }
        }
    }

    #[test]
    fn runtime_ticks_event() {
        let counter = Arc::new(core::sync::atomic::AtomicUsize::new(0));
        let ao_id = ActiveObjectId::new(7);
        let ao = new_active_object(ao_id, 5, Recorder::new(ao_id, Arc::clone(&counter)));

        let mut runtime = Esp32C6QkRuntime::with_builder(
            QkKernel::builder()
                .register(ao)
                .expect("register should succeed"),
            Esp32C6Port::new(),
            PortConfig {
                enable_trace: false,
                tick_hz: 1000,
            },
        )
        .expect("runtime builds");

        let event = Arc::new(TimeEvent::new(
            ao_id,
            TimeEventConfig::new(Signal(42)).with_period(1),
        ));

        runtime.register_time_event(Arc::clone(&event));
        event.arm(1, Some(1));

        runtime.tick().expect("tick succeeds");

        assert_eq!(counter.load(core::sync::atomic::Ordering::SeqCst), 1);
    }
}
