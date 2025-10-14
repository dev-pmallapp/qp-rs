# Quick Reference: Building Examples

## DPP Example for ESP32-C6

### Option 1: From Workspace Root (Recommended for Development)
```bash
cargo build --example dpp --features esp32c6 -p qp-examples --target riscv32imac-unknown-none-elf --release
```

### Option 2: From Examples Directory
```bash
cd examples
cargo build --example dpp --features esp32c6 --release
# Target is set to riscv32imac-unknown-none-elf in .cargo/config.toml
```

### Option 3: Standalone Project (For Flashing)
```bash
cd examples/dpp-esp32c6
cargo build --release
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/dpp-esp32c6
```

## Why Two Approaches?

**Workspace Examples** (`examples/dpp.rs`):
- ✅ Single source file
- ✅ Platform via features
- ✅ Easy to maintain
- ❌ Linker issues (no board-specific linker script)
- **Use for**: Development, testing, CI

**Standalone Projects** (`examples/dpp-esp32c6/`):
- ✅ Complete board package
- ✅ Proper linker scripts
- ✅ Flash directly to hardware
- ❌ Duplicate code
- **Use for**: Deployment, flashing to hardware

## Adding More Platforms

1. Add feature to `examples/Cargo.toml`:
   ```toml
   [features]
   stm32 = ["dep:stm32-hal", ...]
   ```

2. Add conditional dependencies:
   ```toml
   [dependencies]
   stm32-hal = { version = "...", optional = true }
   ```

3. Use in example:
   ```rust
   #[cfg(feature = "stm32")]
   use stm32_hal::*;
   ```

4. Build:
   ```bash
   cargo build --example dpp --features stm32 -p qp-examples
   ```
