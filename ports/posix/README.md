# POSIX port (`qf-port-posix`)

Hosted (desktop) runtime glue for [qp-rs](../../README.md). This is the port used
for development, tests, and emulation — it runs the kernels on a normal OS thread
and streams QS traces over stdout, TCP, or UDP.

## What it provides

- `PosixPort` — owns a QS `Tracer` and exposes a `TraceHook`; can stream records
  to stdout or to a QSpy instance over TCP/UDP.
- `PosixQkRuntime` — wires a `QkKernel` together with a `QkTimerWheel` and the
  tracer for a ready-to-run hosted application.

## Usage

The Dining Philosophers example builds on this port:

```bash
cargo run --bin dpp                       # stdout trace
cargo run --bin qspy -- --tcp localhost:6601   # in another terminal
```

`dpp` connects to `localhost:6601` by default for QSpy tracing.

## Notes

This port maps the QP HAL contract (tick source, trace output, critical section)
onto OS facilities: an OS timer/loop drives `tick()`, and a `std::sync::Mutex`
backs the scheduler critical section.
