# QP Framework Examples

This directory contains example applications demonstrating the QP framework on various platforms.

## Structure

All examples are **standalone projects** in their own directories. This approach is required for embedded targets which need:
- Board-specific linker scripts (memory.x, link.x)
- Interrupt vector tables and startup code
- Platform-specific build scripts
- Proper memory layout definitions

## Available Examples

### Dining Philosophers Problem (DPP)

**Linux**: `dpp-linux/` - Native implementation using POSIX port  
**ESP32-C6**: `dpp-esp32c6/` - Embedded implementation for RISC-V microcontroller

A classic concurrency problem demonstrating:
- 5 philosophers with state machines (thinking/hungry/eating)
- Resource management (fork allocation)
- Deadlock prevention
- Event-driven architecture
- QS software tracing

## Building Examples

### For Linux/POSIX

Native implementation with QS tracing via UDP to QSpy host tool:

```bash
cd examples/dpp-linux
cargo build --release --target x86_64-unknown-linux-gnu

# Terminal 1: Start QSpy host tool
cd ../../tools/qspy
cargo run --release

# Terminal 2: Run DPP example
cd ../../examples/dpp-linux
cargo run --release --target x86_64-unknown-linux-gnu
```

**Note**: QSpy receives traces via UDP on port 7701 and displays formatted output with colored syntax. See `dpp-linux/UDP_QS_INTEGRATION.md` for details.

### For ESP32-C6

Embedded implementation for RISC-V microcontroller:

```bash
cd examples/dpp-esp32c6
cargo build --release
```

### Flashing to Hardware

```bash
cd examples/dpp-esp32c6
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/dpp-esp32c6
```

## Adding New Examples

Create a new standalone project:

```bash
cd examples
cargo new --name myexample myexample-platform
cd myexample-platform
```

Add dependencies to the new project's `Cargo.toml`:
```toml
[dependencies]
qp-core = { path = "../../qp/core" }
qp-qep = { path = "../../qp/qep" }
qp-qf = { path = "../../qp/qf" }
qp-qv = { path = "../../qp/qv" }
# Port-specific dependencies
posix = { path = "../../ports/posix" }  # for Linux
# OR
esp32c6 = { path = "../../ports/esp32c6" }  # for ESP32-C6
```

## Platform Ports

Each example depends on a platform port from `ports/`:

- **POSIX** (`ports/posix/`) - Linux/Unix with std library
  - Critical sections via std::sync::Mutex
  - QS tracing to stdout
  - Used by: `dpp-linux/`

- **ESP32-C6** (`ports/esp32c6/`) - RISC-V embedded middleware
  - Critical sections via interrupt disable
  - QS tracing to UART (planned)
  - Used by: `dpp-esp32c6/`

## Why Standalone Projects?

All examples are standalone projects because embedded targets require:
- Board-specific linker scripts (memory.x, link.x)
- Interrupt vector tables
- Memory layout definitions
- Build scripts for platform-specific code generation

This approach works consistently across all platforms (native and embedded).
