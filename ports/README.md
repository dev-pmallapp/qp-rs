# QP Framework Ports

This directory contains platform-specific ports of the QP framework. Each port is a standalone crate that depends on the core QP workspace crates (`qp-core`, `qp-qep`, `qp-qf`, `qp-qv`, etc.).

## Port Structure

Each port is organized as follows:

```
ports/<platform>/
├── Cargo.toml          # Standalone crate (workspace = [])
├── README.md           # Platform-specific documentation
└── src/
    ├── lib.rs          # Port public API
    ├── critical.rs     # Critical section implementation
    ├── scheduler.rs    # Scheduler integration
    └── time.rs         # Time/tick service
```

## Available Ports

### POSIX (Linux/Unix)

**Location**: `ports/posix/`

Platform for Linux, macOS, and other POSIX-compliant systems using the standard library.

**Features**:
- Thread-safe critical sections using `std::sync::Mutex`
- Drift-free clock tick using monotonic timers
- QV cooperative scheduler integration
- Signal handling (SIGINT/Ctrl-C)

**Build**:
```bash
cd ports/posix
cargo build --release
```

### ESP32-C6 (RISC-V)

**Location**: `ports/esp32c6/`

Bare-metal port for ESP32-C6 microcontroller (RISC-V architecture).

**Features**:
- Critical sections using `esp-hal` primitives
- Hardware timer integration
- No-std embedded environment
- GPIO and peripheral access

**Build**:
```bash
cd ports/esp32c6
cargo build --release
```

## Creating a New Port

To create a port for a new platform:

1. **Create directory structure**:
   ```bash
   mkdir -p ports/<platform>/src
   ```

2. **Create `Cargo.toml`**:
   ```toml
   [package]
   name = "qp-<platform>"
   version = "0.1.0"
   edition = "2021"
   
   # Standalone crate, not part of workspace
   [workspace]
   
   [dependencies]
   qp-core = { path = "../../qp/core" }
   qp-qep = { path = "../../qp/qep" }
   qp-qf = { path = "../../qp/qf" }
   qp-qv = { path = "../../qp/qv" }
   # Add platform-specific HAL crates here
   ```

3. **Implement required components**:
   - **Critical sections**: Platform-specific mutual exclusion
   - **Time service**: Clock tick generation
   - **Scheduler integration**: Event loop and dispatching
   - **Init/cleanup**: Platform startup and shutdown

4. **Add to workspace exclude**:
   Update root `Cargo.toml`:
   ```toml
   [workspace]
   exclude = [
       # ... existing excludes ...
       "ports/<platform>",
   ]
   ```

5. **Test your port**:
   ```bash
   cd ports/<platform>
   cargo test
   ```

## Port Requirements

Each port must provide:

### Critical Section Management

```rust
pub fn enter_critical() -> CriticalSection;
pub fn exit_critical(guard: CriticalSection);
```

### Time Service

```rust
pub fn init();
pub fn set_tick_rate(ticks_per_sec: u32);
pub fn start_ticker();
pub fn stop_ticker();
pub fn register_tick_callback(callback: fn());
```

### Scheduler Integration

```rust
pub fn run() -> !;
pub fn stop();
```

### Initialization

```rust
pub fn init();
pub fn cleanup();
```

## Architecture Benefits

This port structure provides:

- **Separation of Concerns**: Core QP framework is platform-independent
- **Independent Versioning**: Ports can evolve separately from core
- **Flexible Dependencies**: Each port includes only needed platform HALs
- **Easy Testing**: Ports can be tested independently
- **Clear Boundaries**: Explicit interface between framework and platform

## Examples

See the `examples/` directory for applications using different ports:

- `examples/dpp-linux` - Dining Philosophers using `ports/posix`
- `examples/dpp-esp32c6` - Dining Philosophers using `ports/esp32c6`
