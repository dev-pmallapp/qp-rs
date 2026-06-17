# Ports

A **port** is the platform-specific glue that satisfies the QP HAL contract and drives the
kernel run loop. Ports live under `/ports/`.

## The QP HAL contract

The only things QP actually needs from hardware are:

1. **Tick source** — fires at the configured tick rate to call `tick()`.
2. **Trace output** — a byte-stream write path for QS frames (UART, TCP, SWO).
3. **Critical section / interrupt control** — `lock()`/`unlock()` for the scheduler.
4. **Context switch** — PendSV/SVC on Cortex-M (for QXK extended threads).

The `hal/` workspace defines the *peripheral* traits (timer/tick, UART byte-write, SPI,
interrupt control) and stays framework-agnostic — it never depends on `qf`/`qk`. For
peripheral buses prefer the [`embedded-hal`](https://github.com/rust-embedded/embedded-hal)
traits rather than rolling new ones.

## POSIX (`ports/posix`)

The hosted runtime used for development, tests, and emulation. `PosixPort` owns a QS
tracer (stdout/TCP/UDP) and `PosixQkRuntime` wires a `QkKernel` to a `QkTimerWheel`. This
backs `cargo run --bin dpp`.

## Cortex-M (`ports/cortex-m`)

Bare-metal port for the QXK dual-mode kernel with **true context switching**:

- `PendSV` performs the context switch; `SVC #0` is the scheduler-lock primitive.
- `ContextFrame` / `ThreadStack` initialise extended-thread stacks.
- Two build modes via features:
  - `std` *(default)* — hosted/emulation; handlers compile to no-op stubs so desktop
    tests pass.
  - `hw` — real hardware (`no_std`) with the real PendSV/SVC handlers.

```bash
cargo test  -p qf-port-cortex-m                          # hosted
cargo build -p qf-port-cortex-m --no-default-features --features hw
```

## ESP32-S3 / ESP32-C6 (`ports/esp32-s3`, `ports/esp32-c6`)

Embedded targets (Xtensa LX7 and RISC-V respectively). These back the `esp32s3` /
`esp32c6` features of the examples:

```bash
cargo build --bin dpp-esp32-s3 --features esp32s3 --no-default-features
cargo build --bin dpp-esp32-c6 --features esp32c6 --no-default-features
cargo build --bin lora_send_c6 --features esp32c6 --no-default-features
```

Both are works in progress; their READMEs list the integration points (tick timer, QS
transport, interrupt mapping, radio over SPI).

## Porting to a new platform

1. Create `/ports/<platform>/`.
2. Implement a runtime struct holding the kernel + timer wheel.
3. Provide platform initialisation (tick timer, trace transport, critical section).
4. Add a feature flag to the examples for the platform.
