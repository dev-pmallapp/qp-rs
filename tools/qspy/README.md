# QSpy - Software Tracing Host Utilities for Rust QP Framework

This directory contains host-side tools for receiving, interpreting, and displaying QS (Quantum Spy) software tracing output from embedded targets running the QP framework.

## Overview

The QSpy suite consists of three utilities:

1. **qspy** - Main host application that receives and displays QS trace records
2. **qspy-kill** - Utility to terminate a running QSpy instance
3. **qspy-reset** - Utility to reset the target device through QSpy

## Building

```bash
cd tools/qspy
cargo build --release
```

The binaries will be available in `target/release/`:
- `qspy`
- `qspy-kill`
- `qspy-reset`

## Usage

### QSpy Main Application

Start QSpy to listen for trace data:

```bash
# Listen on default port (7701)
./qspy

# Specify custom port
./qspy --port 8888

# Show timestamps
./qspy --timestamps

# Enable verbose mode
./qspy --verbose

# Filter by record groups
./qspy --filter sm ao      # Only show State Machine and Active Object records

# JSON output format
./qspy --format json
```

**Available Filters:**
- `sm` / `statemachine` - State machine transitions
- `ao` / `activeobject` - Active object events
- `eq` / `eventqueue` - Event queue operations
- `mp` / `memorypool` - Memory pool allocations
- `te` / `timeevent` - Time event operations
- `sched` / `scheduler` - Scheduler events
- `sem` / `semaphore` - Semaphore operations
- `mtx` / `mutex` - Mutex operations
- `user` - User-defined records
- `info` - Informational messages
- `dict` / `dictionary` - Dictionary records
- `test` - Test/debugging records
- `err` / `error` - Error records
- `qf` / `framework` - Framework events

### QSpy-Kill

Terminate a running QSpy instance:

```bash
# Kill QSpy on localhost:7701
./qspy-kill

# Kill QSpy on custom host/port
./qspy-kill --qspy 192.168.1.100:7701
```

### QSpy-Reset

Reset the target device through QSpy:

```bash
# Reset target through QSpy on localhost:7701
./qspy-reset

# Reset through QSpy on custom host/port
./qspy-reset --qspy 192.168.1.100:7701
```

## Architecture

### Communication Protocol

QSpy communicates with targets via UDP on port 7701 (default). The protocol consists of:

1. **From Target to QSpy:**
   - Sequence number (1 byte)
   - One or more QS records
   - Each record: type (1 byte) + data (variable length)

2. **From QSpy to Target:**
   - Sequence number (1 byte)
   - Command type (1 byte)
   - Optional payload

### Record Types

QSpy supports all standard QS record types:

- **QEP (Event Processor):** State transitions, dispatches
- **QF (Framework):** Event posting, publishing, memory management
- **Active Objects:** Subscriptions, deferrals, event processing
- **Event Queues:** Post, get, LIFO operations
- **Memory Pools:** Allocations and deallocations
- **Time Events:** Arming, disarming, posting
- **Scheduler:** Preemption, locking, context switches
- **Synchronization:** Semaphores, mutexes
- **User Records:** Custom application traces

### Color Coding

QSpy uses colored output to make trace analysis easier:

- 🔵 **Blue** - State Machine events
- 🟢 **Green** - Active Object events
- 🔵 **Cyan** - Event Queue operations
- 🟣 **Magenta** - Memory Pool operations
- 🟡 **Yellow** - Time Events
- ⚪ **White** - Scheduler events
- 🔴 **Red** - Errors and assertions
- ⚫ **Gray** - Raw hex data

## Integration with Examples

The QSpy tools work with any QP example that has QS tracing enabled:

```bash
# Terminal 1: Start QSpy
cd tools/qspy
cargo run --release

# Terminal 2: Run example with QS tracing
cd examples/dpp-linux
cargo run --release
```

The DPP example will send trace output to stdout (POSIX implementation) or to QSpy via UDP (embedded implementations).

## Differences from C/C++ QSpy

This Rust implementation differs from the original QSpy:

### Advantages:
- ✅ **Memory safe** - No buffer overflows or memory leaks
- ✅ **Modern CLI** - Using `clap` for argument parsing
- ✅ **Async I/O** - Non-blocking UDP reception with Tokio
- ✅ **Colored output** - Better visual parsing of traces
- ✅ **JSON support** - Machine-readable output format
- ✅ **Easy installation** - Single binary, no dependencies

### Limitations (vs C version):
- ⚠️ No serial port support yet (UDP only)
- ⚠️ No MATLAB output
- ⚠️ No sequence diagram generation
- ⚠️ Simplified protocol parser (extensible)

### Planned Features:
- [ ] Serial port (UART) support
- [ ] TCP socket support
- [ ] Binary trace file recording/playback
- [ ] Dictionary management and persistence
- [ ] Sequence diagram output (PlantUML/Mermaid)
- [ ] Integration with test frameworks
- [ ] Web-based UI for trace visualization

## Protocol Extension

The protocol module (`src/protocol.rs`) can be extended to support custom record types:

```rust
use qspy::protocol::{QSRecord, QSRecordType};

// Custom parsing logic
fn parse_custom_record(data: &[u8]) -> QSRecord {
    // Your implementation
}
```

## License

This Rust implementation is dual-licensed under MIT OR Apache-2.0, consistent with the Rust QP framework port.

The original QSpy is licensed under GPL-3.0-or-later OR LicenseRef-QL-commercial by Quantum Leaps.

## Contributing

Contributions welcome! Priority areas:

1. **Serial port support** - Add `serialport` crate integration
2. **Protocol enhancements** - Full QS protocol implementation
3. **Output formats** - CSV, MATLAB, PlantUML diagrams
4. **Performance** - Optimize parser for high-throughput tracing
5. **Documentation** - More examples and use cases

## Resources

- [QP Framework Documentation](https://www.state-machine.com/qp)
- [Original QSpy (C)](https://www.state-machine.com/qtools/qspy.html)
- [QS Protocol Specification](https://www.state-machine.com/qtools/qs.html)
