# HAL: Hardware Abstraction Layer for Embedded Systems

A vendor-agnostic hardware abstraction layer that can be used standalone or integrated with the QP-RS real-time framework.

## Supported Platforms

- **ESP32** (via ESP-IDF) - `hal-esp` crate
- **TI MSP432/TM4C/CC13xx** (via TI DriverLib) - `hal-ti` crate
- **ARM Cortex-M** (via CMSIS) - STM32, NXP, Nordic, etc. - `hal-cmsis` crate
- **Holtek HT32** - `hal-ht32` crate

## Usage

### Standalone (without QP-RS)

```rust
use hal_esp::EspGpioPin;
use hal::gpio::{GpioPin, Level, PinMode};

let mut led = EspGpioPin::new(2).unwrap();
led.set_mode(PinMode::Output).unwrap();
led.write(Level::High).unwrap();
```

### With QP-RS Integration

Enable the `qp-integration` feature to use HAL with QP-RS active objects:

```rust
use hal::integration::KernelEventPoster;

let poster = KernelEventPoster::new(kernel);
// Use in ISR to post events to active objects
```

## Building

```bash
# Build core HAL traits
cargo build

# Build vendor-specific HAL
cargo build -p hal-esp --features esp32s3
cargo build -p hal-cmsis --features stm32f4xx

# Build with QP integration
cargo build --features qp-integration
```

## Architecture

- `hal` (root package) - Core trait definitions
- `hal-esp` - ESP-IDF implementation
- `hal-ti` - TI DriverLib implementation
- `hal-cmsis` - ARM CMSIS implementation
- `hal-ht32` - Holtek HT32 implementation

See individual crate documentation for platform-specific details.
