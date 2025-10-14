# QP Rust Tools

This directory contains host-side development and debugging tools for the QP framework.

## Available Tools

### QSpy - Software Tracing Suite

The QSpy suite provides real-time software tracing capabilities for embedded targets:

- **`qspy`** - Main host application that receives and displays QS trace records from targets
- **`qspy-kill`** - Utility to terminate a running QSpy instance  
- **`qspy-reset`** - Utility to reset the target device through QSpy

See [`qspy/README.md`](qspy/README.md) for detailed documentation.

## Quick Start

### Build All Tools

```bash
cargo build --release -p qspy
```

Binaries will be in `target/release/`:
- `qspy`
- `qspy-kill`
- `qspy-reset`

### Run QSpy

```bash
# Start listening for traces
./target/release/qspy

# With options
./target/release/qspy --timestamps --filter sm ao
```

### Example Workflow

**Terminal 1 - Start QSpy:**
```bash
cd /path/to/qp-rs
./target/release/qspy --timestamps --verbose
```

**Terminal 2 - Run Example:**
```bash
cd examples/dpp-linux
cargo run --release
```

The example will output traces to stdout (POSIX implementation) or send them via UDP to QSpy (embedded implementations).

## Tool Comparison

### Rust vs Original C/C++ QSpy

| Feature | Rust | C/C++ |
|---------|------|-------|
| UDP tracing | ✅ | ✅ |
| Serial port | ⚠️ Planned | ✅ |
| TCP socket | ⚠️ Planned | ✅ |
| Memory safety | ✅ | ⚠️ |
| Colored output | ✅ | Limited |
| JSON export | ✅ | ❌ |
| MATLAB export | ⚠️ Planned | ✅ |
| Sequence diagrams | ⚠️ Planned | ✅ |
| Dictionary management | ⚠️ Planned | ✅ |
| Cross-platform | ✅ | ✅ |

## Future Tools

Planned additions to the tools directory:

- **qcalc** - Quantum Calculator for real-time calculations
- **qfsgen** - File system image generator
- **qview** - GUI trace visualizer
- **qutest** - Unit testing framework integration
- **qclean** - Project cleanup utility

## Development

### Adding New Tools

1. Create a new directory in `tools/`:
```bash
mkdir tools/mytool
cd tools/mytool
cargo init
```

2. Add to workspace in root `Cargo.toml`:
```toml
[workspace]
members = [
    # ... existing members ...
    "tools/mytool",
]
```

3. Implement your tool

4. Update this README

### Dependencies

Tools are allowed to use std library and have richer dependencies than the embedded framework:

- **clap** - Command-line argument parsing
- **tokio** - Async runtime
- **serde** - Serialization
- **colored** - Terminal colors
- **anyhow** - Error handling

## Resources

- [QP Framework Documentation](https://www.state-machine.com/qp)
- [QTools Suite](https://www.state-machine.com/qtools)
- [QS Protocol Specification](https://www.state-machine.com/qtools/qs.html)

## License

Rust implementations: MIT OR Apache-2.0

Original QTools: GPL-3.0-or-later OR LicenseRef-QL-commercial by Quantum Leaps
