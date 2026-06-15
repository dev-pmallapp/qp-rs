# Getting Started

## Prerequisites

- A recent stable Rust toolchain (`rustup`, `cargo`).
- For embedded targets, the relevant target(s) and ESP toolchain (see [Ports](./ports.md)).

## Build and test

```bash
# Build the whole workspace
cargo build

# Run all tests
cargo test

# Build a single crate
cargo build -p qf
```

## Run the Dining Philosophers example

The DPP example exercises multiple active objects (a Table plus five Philosophers), time
events, and QS tracing:

```bash
cargo run --bin dpp
```

By default `dpp` also connects to a QSpy instance on `localhost:6601`. To watch the trace,
run QSpy in another terminal first:

```bash
cargo run --bin qspy -- --tcp localhost:6601
```

## Run the QXK examples

```bash
cargo run --example sync_primitives
cargo run --example producer_consumer
```

## Run the LoRa example (host / simulated radio)

```bash
cargo run --bin lora_send
```

## Where to go next

- New to the model? Read **[Concepts](./concepts.md)**.
- Choosing a kernel? See **[Kernels](./kernels.md)**.
- Targeting hardware? See **[Ports](./ports.md)**.
