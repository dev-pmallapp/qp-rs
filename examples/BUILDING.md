# Quick Reference: Building Examples

All examples are **standalone projects** in their own directories.

## DPP Example for Linux (POSIX)

Native implementation with QS software tracing via UDP to QSpy host tool:

### Two-Terminal Workflow

**Terminal 1 - Start QSpy:**
```bash
cd tools/qspy
cargo run --release
# QSpy will listen on UDP port 7701 for traces
```

**Terminal 2 - Run DPP:**
```bash
cd examples/dpp-linux
cargo build --release --target x86_64-unknown-linux-gnu
cargo run --release --target x86_64-unknown-linux-gnu
```

### Without QS Tracing (Zero Overhead)

```bash
cd examples/dpp-linux
cargo build --release --target x86_64-unknown-linux-gnu --no-default-features
cargo run --release --target x86_64-unknown-linux-gnu --no-default-features
```

### QSpy Output

QSpy provides real-time formatted trace output:
- Colored output by trace category
- State machine transitions
- Event posting and dispatching
- Timing information

See `examples/dpp-linux/UDP_QS_INTEGRATION.md` for complete documentation.

## DPP Example for ESP32-C6

Embedded RISC-V implementation:

```bash
cd examples/dpp-esp32c6
cargo build --release
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/dpp-esp32c6
```

## Why Standalone Projects?

All examples (native and embedded) use standalone project directories because:

**For Embedded Targets** (ESP32-C6, STM32, etc.):
- Require board-specific linker scripts (memory.x, link.x)
- Need interrupt vector tables and startup code
- Depend on build.rs that generates platform-specific files
- Must define memory layouts

**For Native Targets** (Linux, etc.):
- Consistency with embedded approach
- Isolated dependencies per platform
- Easier to test and maintain
- No feature flag complexity

## Project Structure

Each example is a complete standalone project:

```
examples/
├── dpp-linux/           # Native Linux implementation
│   ├── Cargo.toml       # Dependencies on qp/* and ports/posix
│   └── src/main.rs      # Application code
└── dpp-esp32c6/         # Embedded ESP32-C6 implementation
    ├── Cargo.toml       # Dependencies on qp/* and ports/esp32c6
    ├── build.rs         # Platform-specific build script
    ├── .cargo/config.toml
    └── src/main.rs      # Application code
```

## Adding New Examples

Create a new standalone project:

```bash
cd examples
cargo new --name myexample myexample-platform
cd myexample-platform
```

Add QP dependencies to `Cargo.toml`:

```toml
[dependencies]
qp-core = { path = "../../qp/core" }
qp-qep = { path = "../../qp/qep" }
qp-qf = { path = "../../qp/qf" }
qp-qv = { path = "../../qp/qv" }

# Choose appropriate port
posix = { path = "../../ports/posix" }         # For Linux/Unix
# OR
esp32c6 = { path = "../../ports/esp32c6" }     # For ESP32-C6
```

## Platform Ports

Examples depend on middleware ports from `ports/`:

- `ports/posix/` - Linux/Unix with std (stdout QS tracing)
- `ports/esp32c6/` - ESP32-C6 RISC-V (UART QS tracing)
