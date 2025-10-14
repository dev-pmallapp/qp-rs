# QP Framework Examples

This directory contains example applications demonstrating the QP framework on various platforms.

## Structure

Examples are organized as a workspace member with platform-specific features. This allows:
- Single source file per example
- Platform selection via features
- Reusable across different boards/OSes

## Available Examples

### Dining Philosophers Problem (DPP)

**Source**: `dpp.rs` (reference), `dpp-esp32c6/` (buildable)  
**Platforms**: ESP32-C6 (RISC-V)

⚠️ **For ESP32-C6, use the standalone project `dpp-esp32c6/`** - the workspace example won't link.

A classic concurrency problem demonstrating:
- 5 philosophers with state machines (thinking/hungry/eating)
- Resource management (fork allocation)
- Deadlock prevention
- Event-driven architecture

## Building Examples

### For ESP32-C6

**Use the standalone project** (workspace examples don't link for embedded targets):

```bash
cd examples/dpp-esp32c6
cargo build --release
```

### Flashing to Hardware

The standalone projects (e.g., `dpp-esp32c6/`) are complete board-specific packages with proper linker scripts:

```bash
cd examples/dpp-esp32c6
cargo build --release
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/dpp-esp32c6
```

## Adding New Examples

1. Create example file: `examples/myexample.rs`
2. Add to `Cargo.toml`:
   ```toml
   [[example]]
   name = "myexample"
   path = "myexample.rs"
   required-features = ["platform"]
   ```
3. Add platform-specific dependencies as optional
4. Use `#![cfg(feature = "platform")]` in example code

## Platform Features

Currently available:
- `esp32c6` - ESP32-C6 RISC-V microcontroller (use `dpp-esp32c6/` standalone project)

Future (for hosted/native examples):
- `linux` - Native Linux examples with std
- `windows` - Native Windows examples with std

## Important Notes

### Why Standalone Projects for Embedded?

Embedded targets (ESP32-C6, STM32, etc.) **cannot use workspace examples** because they need:
- Board-specific linker scripts (memory.x, link.x)
- Interrupt vector tables
- Memory layout definitions
- Build scripts that generate platform-specific code

**Solution**: Use standalone project directories like `dpp-esp32c6/` which have complete build setups.

### When Will Workspace Examples Work?

The workspace examples approach (`examples/dpp.rs` with features) will work for:
- Native/hosted platforms (Linux, Windows, macOS) with `std`
- QEMU simulation targets
- Test environments

For now, `dpp.rs` serves as a reference implementation that's kept in sync with the standalone projects.
