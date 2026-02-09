# HAL: Hardware Abstraction Layer for Embedded Systems

A vendor-agnostic hardware abstraction layer that can be used standalone or integrated with the QP-RS real-time framework.

## Features

- **Vendor-Agnostic API**: Write portable embedded code using trait-based abstractions
- **Comprehensive Peripherals**: GPIO, UART, SPI, I2C, Timers, PWM, ADC, DAC, Interrupts
- **Safe FFI Wrappers**: All unsafe vendor C calls wrapped in safe Rust APIs
- **Optional QP Integration**: Event posting from ISRs to active objects
- **no_std Compatible**: Works in bare-metal embedded environments
- **Zero-Cost Abstractions**: Trait dispatch with minimal overhead

## Supported Platforms

| Platform | Crate | Status | Features |
|----------|-------|--------|----------|
| **ESP32 Family** | `hal-esp` | ✅ Implemented | GPIO, UART, SPI (ESP-IDF) |
| **TI MCUs** | `hal-ti` | 🚧 Placeholder | MSP432, TM4C, CC13xx |
| **ARM Cortex-M** | `hal-cmsis` | 🚧 Placeholder | STM32, nRF, LPC (CMSIS) |
| **Holtek HT32** | `hal-ht32` | 🚧 Placeholder | HT32 series |

## Quick Start

### Standalone Usage (No QP-RS)

```rust
use hal_esp::{EspGpioPin, EspUart};
use hal::gpio::{GpioPin, Level, PinMode};
use hal::uart::{UartPort, UartConfig};

// GPIO Example
let mut led = EspGpioPin::new(2).unwrap();
led.set_mode(PinMode::Output).unwrap();
led.toggle().unwrap();

// UART Example
let mut uart = EspUart::new(1).unwrap();
uart.configure(&UartConfig::default()).unwrap();
uart.write(b"Hello, HAL!\n").unwrap();
```

### With QP-RS Integration

```rust
use hal::integration::KernelEventPoster;
use hal_esp::EspGpioPin;
use hal::gpio::{GpioPinInterrupt, Edge};

// Create event poster for kernel
let poster = KernelEventPoster::new(kernel);

// Configure GPIO interrupt that posts to active object
let mut button = EspGpioPin::new(0).unwrap();
button.enable_interrupt(Edge::Falling).unwrap();

// In ISR handler:
poster.post_event(button_ao_id, signal, event).unwrap();
```

## Building

### Core HAL Traits
```bash
cd hal
cargo build                           # No vendor dependencies
cargo build --features qp-integration # With QP-RS integration
cargo doc --open                      # View trait documentation
```

### ESP32 Implementation
```bash
cargo build -p hal-esp --features esp32s3
cargo test -p hal-esp
```

### Examples
```bash
# GPIO blink (portable across platforms)
cargo check -p gpio-blink --features esp32s3

# UART echo
cargo check -p uart-echo --features esp32s3
```

## Project Structure

```
hal/
├── src/                  # Core HAL traits (root package)
│   ├── gpio.rs          # GPIO abstraction
│   ├── uart.rs          # UART abstraction
│   ├── spi.rs           # SPI abstraction
│   ├── i2c.rs           # I2C abstraction
│   ├── timer.rs         # Timer/PWM abstraction
│   ├── adc.rs           # ADC abstraction
│   ├── dac.rs           # DAC abstraction
│   ├── interrupt.rs     # Interrupt controller
│   ├── error.rs         # Common error types
│   └── integration.rs   # QP-RS integration (optional)
│
├── hal-esp/             # ESP-IDF implementation
│   ├── src/
│   │   ├── gpio.rs     # ✅ ESP32 GPIO with interrupts
│   │   ├── uart.rs     # ✅ ESP32 UART with DMA
│   │   └── spi.rs      # ✅ ESP32 SPI master
│   └── build.rs        # ESP-IDF build integration
│
├── hal-ti/              # TI DriverLib (placeholder)
├── hal-cmsis/           # ARM CMSIS (placeholder)
├── hal-ht32/            # Holtek (placeholder)
│
└── examples/
    ├── gpio-blink/      # Portable GPIO example
    └── uart-echo/       # Portable UART example
```

## Peripheral Support Matrix

| Peripheral | Trait | ESP32 | TI | CMSIS | HT32 |
|------------|-------|-------|----|----|------|
| GPIO | `GpioPin` | ✅ | 🚧 | 🚧 | 🚧 |
| GPIO Interrupt | `GpioPinInterrupt` | ✅ | 🚧 | 🚧 | 🚧 |
| UART | `UartPort` | ✅ | 🚧 | 🚧 | 🚧 |
| SPI Master | `SpiMaster` | ✅ | 🚧 | 🚧 | 🚧 |
| I2C Master | `I2cMaster` | 🚧 | 🚧 | 🚧 | 🚧 |
| Timer | `Timer` | 🚧 | 🚧 | 🚧 | 🚧 |
| PWM | `PwmChannel` | 🚧 | 🚧 | 🚧 | 🚧 |
| ADC | `AdcChannel` | 🚧 | 🚧 | 🚧 | 🚧 |
| DAC | `DacChannel` | 🚧 | 🚧 | 🚧 | 🚧 |

✅ = Implemented | 🚧 = Planned

## Design Philosophy

### Safety First
- All FFI calls wrapped in safe public APIs
- Proper error handling with `Result` types
- RAII cleanup (Drop trait) for resource management

### Portability
- Vendor-agnostic trait definitions
- Consistent API across all platforms
- Feature-gated vendor implementations

### Performance
- Zero-cost abstractions via trait dispatch
- Inline-friendly implementations
- No unnecessary allocations

### Integration
- Optional QP-RS active object integration
- ISR-safe event posting with scheduler locks
- Trace integration for diagnostics

## Examples

See `/examples` directory for complete working examples:

- **gpio-blink**: Portable GPIO blink example
- **uart-echo**: UART read/write with timeout

## Contributing

To add support for a new platform:

1. Create `hal-{vendor}/` directory
2. Implement traits from `hal` crate
3. Add FFI bindings to vendor C library
4. Add feature flags for chip variants
5. Create examples demonstrating usage

See `hal-esp` as a reference implementation.

## License

MIT OR Apache-2.0 (same as QP-RS)
