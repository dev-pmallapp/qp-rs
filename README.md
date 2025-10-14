# QP Framework Rust Port

This is a Rust port of the QP real-time embedded framework, originally developed by Quantum Leaps in C/C++.

## Current Status

This project is in early development. The following components have been implemented:

### ✅ Completed (Phase 1 - Foundation)

- [x] **Project Structure**: Cargo workspace with separate crates
- [x] **Core Types**: Event types, signals, state machine types
- [x] **Priority System**: Type-safe priority handling with masks
- [x] **Time Management**: Time events, durations, and tick counting
- [x] **Memory Management**: Static memory pools and event allocation

### 🚧 In Progress

- [ ] **Event Processing Engine (QEP)**: State machine implementation
- [ ] **Framework Layer (QF)**: Active object traits and management
- [ ] **Real-Time Kernels**: QV, QK, QXK scheduler implementations

### 📋 Planned

- [ ] **Platform Ports**: ARM Cortex-M, RISC-V support
- [ ] **Examples**: Blinky, Dining Philosophers Problem
- [ ] **Testing Framework**: Unit tests and integration tests
- [ ] **Documentation**: API docs and tutorials

### Architecture

The framework is organized into several crates:

- **`qp-core`**: Core types, events, states, priorities, and time management
- **`qp-mem`**: Memory management with static pools and event allocation  
- **`qp-qep`**: Event Processing Engine (QEP) - state machines
- **`qp-qf`**: Framework (QF) - active objects and event management
- **`qp-qv`**: Vanilla kernel (QV) - cooperative scheduling
- **`qp-qk`**: Preemptive kernel (QK) - priority-based preemption
- **`qp-qxk`**: Extended kernel (QXK) - dual-mode scheduling
- **`qp-qs`**: Spy (QS) - software tracing infrastructure
- **`qp-bsp`**: Board Support Package abstractions

### Design Principles

This Rust port maintains the real-time deterministic behavior of the original while leveraging Rust's advantages:

- **Memory Safety**: Zero runtime panics in well-formed programs
- **Zero-Cost Abstractions**: Compile-time optimizations with no overhead
- **Type Safety**: Prevents common state machine design errors at compile time
- **`no_std` Compatible**: Suitable for bare-metal embedded targets

### Getting Started

#### Option 1: Dev Container (Recommended)

The easiest way to get started is using VS Code with Dev Containers:

1. Install [VS Code](https://code.visualstudio.com/) and the [Dev Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers)
2. Install [Docker Desktop](https://www.docker.com/products/docker-desktop/)
3. Open this project in VS Code
4. Press `Ctrl/Cmd + Shift + P` and select "Dev Containers: Rebuild and Reopen in Container"
5. Wait for the container to build and initialize (first time takes ~5-10 minutes)

The dev container includes:
- Complete Rust toolchain with embedded targets
- probe-rs for embedded debugging and flashing
- All VS Code extensions for Rust development
- Pre-configured build and test scripts

#### Option 2: Local Installation

Alternatively, install the Rust toolchain locally:

```bash
# Install rustup (Rust installer and version manager)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add embedded targets
rustup target add thumbv7em-none-eabihf  # ARM Cortex-M4F
rustup target add thumbv6m-none-eabi     # ARM Cortex-M0
rustup target add riscv32imac-unknown-none-elf  # RISC-V
```

#### Building and Testing

Build the project:

```bash
cargo build
```

Run tests:

```bash
cargo test
# Or use the comprehensive test script (dev container)
./scripts/test-all.sh
```

Run the blinky example:

```bash
cargo run --example blinky
```

Build for embedded targets:

```bash
cargo build --target thumbv7em-none-eabihf  # ARM Cortex-M4F
# Or build all targets (dev container)
./scripts/build-all-targets.sh
```

### Contributing

This project follows the task breakdown outlined in `.github/copilot-instructions.md`.

See the GitHub instructions for the detailed development roadmap and contribution guidelines.