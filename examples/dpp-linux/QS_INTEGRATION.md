# QS Tracing Integration for DPP Linux Example

## Summary

Successfully integrated QS (Quantum Spy) software tracing into the DPP Linux example with optional compilation support, matching the pattern used in the original QP/C++ implementation.

## Changes Made

### 1. Added QS Feature Flag (`examples/dpp-linux/Cargo.toml`)

```toml
[features]
default = ["qs"]        # QS enabled by default
qs = ["dep:qp-qs"]      # Enable QS software tracing

[dependencies]
# ... other dependencies ...
qp-qs = { path = "../../qp/qs", features = ["std"], optional = true }
```

### 2. Signal Dictionary Implementation

Added signal naming and dictionary production similar to C++ implementation:

```rust
impl DPPSignal {
    /// Get signal name for debugging/tracing
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Hungry => "HUNGRY_SIG",
            Self::Done => "DONE_SIG",
            Self::Eat => "EAT_SIG",
            Self::Timeout => "TIMEOUT_SIG",
        }
    }
}

#[cfg(feature = "qs")]
fn produce_sig_dict() {
    println!("[QS] Signal Dictionary:");
    println!("  {} = {}", DPPSignal::Eat as u16, DPPSignal::Eat.name());
    println!("  {} = {}", DPPSignal::Done as u16, DPPSignal::Done.name());
    println!("  {} = {}", DPPSignal::Timeout as u16, DPPSignal::Timeout.name());
    println!("  {} = {}", DPPSignal::Hungry as u16, DPPSignal::Hungry.name());
}
```

This matches the C++ pattern:
```cpp
#ifdef Q_SPY
inline void produce_sig_dict() {
    QS_SIG_DICTIONARY(EAT_SIG,     nullptr);
    QS_SIG_DICTIONARY(DONE_SIG,    nullptr);
    QS_SIG_DICTIONARY(PAUSE_SIG,   nullptr);
    QS_SIG_DICTIONARY(SERVE_SIG,   nullptr);
    QS_SIG_DICTIONARY(TEST_SIG,    nullptr);
    QS_SIG_DICTIONARY(TIMEOUT_SIG, nullptr);
    QS_SIG_DICTIONARY(HUNGRY_SIG,  nullptr);
}
#endif
```

### 3. Conditional QS Compilation

All QS-related code is now wrapped with `#[cfg(feature = "qs")]`:

- **Imports**: `use qp_qs::{self as qs, QSRecordType};`
- **Initialization**: QS init, enable, and signal dictionary
- **State Machine Tracing**: `qs::qs_sm_tran!()` macro calls
- **Manual Tracing**: `qs::begin()`, `qs::u8()`, `qs::str()`, `qs::end()`
- **Buffer Flushing**: `qs::flush()`

Example in state handlers:
```rust
fn thinking(me: &mut dyn QStateMachine, e: &dyn QEvent) -> QStateReturn {
    let sig_val = e.signal().0;
    if sig_val == DPPSignal::Timeout as u16 {
        // Trace the transition (only if QS enabled)
        #[cfg(feature = "qs")]
        qs::qs_sm_tran!(me, Self::thinking, Self::hungry);
        
        QStateReturn::Transition(Self::hungry)
    } else {
        QStateReturn::Super(Self::top)
    }
}
```

### 4. Runtime Status Display

The application now shows QS status at startup:

**With QS (default):**
```
╔════════════════════════════════════════╗
║  QP Framework - Dining Philosophers    ║
║  Running on Linux (POSIX)              ║
║  QS Tracing: ENABLED                   ║
╚════════════════════════════════════════╝

[QS] Signal Dictionary:
  3 = EAT_SIG
  2 = DONE_SIG
  4 = TIMEOUT_SIG
  1 = HUNGRY_SIG
```

**Without QS:**
```
╔════════════════════════════════════════╗
║  QP Framework - Dining Philosophers    ║
║  Running on Linux (POSIX)              ║
║  QS Tracing: DISABLED                  ║
╚════════════════════════════════════════╝
```

## Usage

### Build with QS (Default)

```bash
cd examples/dpp-linux
cargo build --release --target x86_64-unknown-linux-gnu
cargo run --release --target x86_64-unknown-linux-gnu
```

### Build without QS

```bash
cd examples/dpp-linux
cargo build --release --target x86_64-unknown-linux-gnu --no-default-features
cargo run --release --target x86_64-unknown-linux-gnu --no-default-features
```

### Check Binary Size

```bash
# With QS
ls -lh target/x86_64-unknown-linux-gnu/release/dpp-linux

# Without QS (build with --no-default-features first)
ls -lh target/x86_64-unknown-linux-gnu/release/dpp-linux
```

## Benefits

### 1. **Zero-Cost Abstraction**
When QS is disabled, all tracing code is compiled out completely:
- No runtime overhead
- Smaller binary size
- No unused dependencies

### 2. **Debug vs Release Flexibility**
- **Development**: Enable QS for detailed state machine traces
- **Production**: Disable QS for minimal overhead and binary size

### 3. **Pattern Consistency**
Matches the C/C++ QP framework's `#ifdef Q_SPY` pattern:
```cpp
#ifdef Q_SPY
    // C++ tracing code
#endif
```

becomes:
```rust
#[cfg(feature = "qs")]
{
    // Rust tracing code
}
```

### 4. **Signal Dictionary**
Makes trace output human-readable by mapping signal numbers to names:
- `1 = HUNGRY_SIG`
- `2 = DONE_SIG`
- `3 = EAT_SIG`
- `4 = TIMEOUT_SIG`

## Code Locations

### Tracing Points

1. **State Handler Transitions** (`src/main.rs:131-169`)
   - `Philosopher::thinking()` → traces THINKING→HUNGRY
   - `Philosopher::hungry()` → traces HUNGRY→EATING
   - `Philosopher::eating()` → traces EATING→THINKING

2. **Simulation Loop Events** (`src/main.rs:333-393`)
   - Manual `qs::begin(QS_SM_TRAN)` traces with additional data
   - Philosopher ID, state names, cycle counts

3. **Periodic Flush** (`src/main.rs:413-417`)
   - Every 100 cycles, flush QS buffer to stdout

## Future Enhancements

### 1. UDP Output
Currently outputs to stdout (POSIX port). Can be enhanced to send via UDP to QSpy host tool:

```rust
#[cfg(feature = "qs-udp")]
fn send_to_qspy(record: &[u8]) {
    // Send UDP packet to localhost:7701
}
```

### 2. Additional Dictionaries
Following the C++ pattern, add:
- Object dictionary (philosopher instances, table)
- Function dictionary (state handlers)
- User dictionary (custom records)

### 3. More Trace Points
Add tracing for:
- Fork allocation/deallocation
- Queue operations
- Timer events
- Memory pool operations

### 4. Performance Metrics
With QS enabled, track:
- Average thinking time
- Average eating time
- Fork contention statistics
- State transition frequencies

## Testing

Both configurations compile and run successfully:

### With QS (Default)
```bash
$ cargo run --release
...
[QS] Signal Dictionary:
  3 = EAT_SIG
  2 = DONE_SIG
  4 = TIMEOUT_SIG
  1 = HUNGRY_SIG

[51] Philosopher 0 thinking -> HUNGRY (thought for 50 cycles)
[51] Philosopher 0 got forks -> EATING
[QS:00000001] SM_TRAN          ...
```

### Without QS
```bash
$ cargo run --release --no-default-features
...
QS Tracing: DISABLED

[51] Philosopher 0 thinking -> HUNGRY (thought for 50 cycles)
[51] Philosopher 0 got forks -> EATING
# No QS trace records
```

## Comparison with C++ Implementation

| Feature | C++ (Q_SPY) | Rust (qs feature) |
|---------|-------------|-------------------|
| Conditional compilation | `#ifdef Q_SPY` | `#[cfg(feature = "qs")]` |
| Signal dictionary | `QS_SIG_DICTIONARY()` | `produce_sig_dict()` |
| State transitions | `QS_STATE_TRAN()` | `qs::qs_sm_tran!()` |
| Manual records | `QS_BEGIN()` / `QS_END()` | `qs::begin()` / `qs::end()` |
| Initialization | `QS_INIT()` | `qs::init()` |
| Buffer flush | `QS_FLUSH()` | `qs::flush()` |
| Zero overhead | ✅ When disabled | ✅ When disabled |

## Documentation Updates

Updated files:
- ✅ `examples/dpp-linux/src/main.rs` - Added QS feature support
- ✅ `examples/dpp-linux/Cargo.toml` - Added feature flags
- ✅ Header comments - Build instructions for both modes

## Conclusion

The DPP Linux example now fully supports optional QS software tracing, matching the architecture and pattern of the original QP/C++ framework. This provides flexibility for development (with tracing) and production (without overhead) while maintaining zero-cost abstractions when QS is disabled.
