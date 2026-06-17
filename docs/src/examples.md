# Examples

## Dining Philosophers (DPP) — `examples/dpp`

The canonical QP example. It demonstrates:

- multiple active objects: a `Table` plus five `Philosopher`s,
- time events for thinking/eating timeouts,
- QEP/HSM state-machine records and custom user records,
- multi-platform builds: host (POSIX), ESP32-S3, ESP32-C6.

```bash
cargo run --bin dpp                       # host
cargo run --bin qspy -- --tcp localhost:6601   # watch the trace
```

The `examples/dpp/tests/qutest_dpp.rs` integration test drives the example through the QS
test-probe machinery.

## LoRa send — `examples/lora_send`

Exercises the full App → `comms` → `hal` → radio chain:

- the host target simulates the radio (`NullRf`),
- the ESP32-C6 target uses real SX127x/SX126x hardware,
- shows how QF active objects drive the RF middleware.

```bash
cargo run --bin lora_send                                   # host
cargo build --bin lora_send_c6 --features esp32c6 --no-default-features
```

See the [RF stack plan](https://github.com/) (`RF_STACK_PLAN.md` in the repo root) for how
the `comms` layer is intended to grow into a full radio-agnostic protocol stack.

## QXK synchronization — `crates/qxk/examples`

- `sync_primitives.rs` — semaphores, mutexes, message queues, condition variables.
- `producer_consumer.rs` — thread coordination with blocking primitives.

```bash
cargo run --example sync_primitives
cargo run --example producer_consumer
```
