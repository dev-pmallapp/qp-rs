# QP-RS Agent Instructions & Knowledge Base

This file serves as the primary entry point and memory store for AI agents (Gemini, Claude Code, Cursor, Cline, etc.) working on the `qp-rs` repository. Read this first before implementing any changes.

---

## 1. Project Overview & Architecture

**QP-RS** is a Rust port of the Quantum Platform (QP) real-time embedded framework. It implements active object-based, event-driven architectures with cooperative (`QvKernel`) and preemptive (`QkKernel`) kernels, alongside a diagnostics/tracing system (`QS`) and an extended dual-mode kernel (`Qxk`) supporting blocking threads.

### Workspace Layout

The repository is structured as a cargo workspace containing the core framework crates, target ports, and a separate, independent `hal` sub-workspace:

```
/
├── crates/
│   ├── qp-rs/          # Unified facade crate (main downstream dependency)
│   ├── qf/             # Quantum Framework (Active Objects, cooperative kernel)
│   ├── qk/             # Quantum Kernel (Preemptive scheduling)
│   ├── qxk/            # Quantum eXtended Kernel (Extended blocking threads)
│   └── qs/             # Quantum Spy (Diagnostics, binary tracing protocol)
│   └── comms/          # LoRa/LoRaWAN and FOTA protocol stack
├── hal/                # Independent sub-workspace (excluded from root Cargo.toml)
│   ├── hal/            # Core framework-agnostic peripheral traits
│   ├── hal-cmsis/      # Cortex-M register/interrupt helpers
│   ├── hal-lxsis/      # Xtensa LX register/interrupt helpers
│   └── hal-rvsis/      # RISC-V register/interrupt helpers
├── ports/              # Target-specific platform runtime glue
│   ├── posix/          # POSIX host runtime
│   ├── esp32-s3/       # ESP32-S3 Espressif target
│   ├── esp32-c6/       # ESP32-C6 Espressif target
│   ├── cortex-m/       # Cortex-M bare-metal target
│   ├── riscv/          # Generic RISC-V target
│   └── xtensa/         # Generic Xtensa target
├── examples/           # Integration examples (dpp, lora_send)
├── configs/            # Build configs & Renode simulation files
└── tools/              # Host tools (qspy, etc.)
```

### The Facade Crate (`qp-rs`)
Downstream projects should depend only on the unified `qp-rs` facade crate. It re-exports all framework crates via feature gates:
- `qk`: Enables the preemptive single-stack kernel (includes `qf`).
- `qxk`: Enables the dual-mode kernel with blocking threads (includes `qf` + `qk`).
- `qs`: Enables QS tracing (propagates to active crates).
- `comms`: Enables LoRa/LoRaWAN & FOTA middleware.
- `hal`: Enables framework-agnostic peripheral traits.
- `std`: Propagates standard library support (defaults to `no_std`).
- `smp`: Enables Symmetric Multiprocessing (SMP) support across kernels (e.g. QvKernel).

---

## 2. Layering & Dependency Rules (Strict)

The codebase enforces a strict one-way dependency hierarchy that **must not be inverted**:

```
comms / examples          (protocol middleware, application)
       ↓ uses
qf / qk / qxk / qs       (framework — active objects, events, tracing)
       ↓ uses
hal                       (hardware abstraction traits — framework-agnostic)
       ↓ uses
hal-esp / hal-cmsis / …   (chip-specific implementations)
```

### Key Rules
1. **`hal/` must remain framework-agnostic.** It must *never* depend on `qf`, `qk`, or any other framework crate. It only knows about basic peripheral traits, critical sections, and clock/timer interfaces.
2. **`comms` belongs in the main workspace, not `hal/`.** It drives LoRa/LoRaWAN workflows via QF active objects and events, meaning it depends on the framework layer. Moving it into the `hal/` workspace would cause a dependency inversion.
3. **Use standard traits for peripherals.** For generic peripherals (SPI, UART, GPIO, I2C), favor `embedded-hal` 1.0 and `embedded-io` 0.6 traits rather than defining custom traits in `hal/`.

---

## 3. Platform Ports & Runtime Model

The target-specific port crates (in `ports/`) bridge the hardware with the framework using three traits defined in `qf::port`:
1. **`Runtime`**: Uniform driver interface exposing `tick()`, `run_until_idle()`, and `has_pending_work()`.
2. **`TraceSink`**: Provides a `TraceHook` to stream binary QS data to the platform's diagnostics channel (UART, USB JTAG, TCP, etc.).
3. **`ContextSwitch`**: Requests an asynchronous context switch (e.g., pending `PendSV` on Cortex-M or writing `MSIP0` on RISC-V CLINT).

### Porting & Platform Details
- **Cooperative Targets**: Context switching is a no-op (`NoopContextSwitch`); execution switches synchronously at run-to-completion boundaries.
- **Bare-Metal Preemption**: Managed by low-level assembly trap handlers (e.g. `TrapHandler` in `ports/riscv/src/lib.rs`) which save/restore CPU registers and delegate scheduling decisions to `qxk_schedule()` or `qk::isr_exit()`.

---

## 4. Key Architectural Decisions & Hardware Constraints

### HAL Migration "Safe-Faithful" Path
When integrating or updating target hardware (such as ESP32-C6), prioritize reliability over strict abstract alignment:
- **No CLINT on ESP32-C6**: The generic `hal-rvsis` crate features a `ClintTimer` that maps to the standard RISC-V CLINT memory-mapped registers (at `0x0200_0000`). However, the ESP32-C6 uses Espressif's custom SYSTIMER instead of a standard CLINT. Attempting to use the `ClintTimer` on ESP32-C6 reads garbage memory, corrupting system timing and silencing the firmware. **Keep the ESP32-C6 port bound to `esp_hal` for system time and timing.**
- **USB Serial/JTAG Console**: The standard `EspQsSink` (diagnostics channel) writes to the USB Serial/JTAG peripheral for virtual console output. `hal-rvsis` only supports a standard hardware UART (`Esp32C6Uart`). Do not force migration to the `hal-rvsis` UART if it breaks USB-serial-based QS tracing.
- **`SpiMaster` vs `SpiBus`**: Keep drivers on standard `embedded_hal::spi::SpiBus`. The `SpiMaster` trait in `qp-rs/hal` is an internal implementation helper, not a public driver API.

### Interrupt Service Routine (ISR) & DMA Model
To prevent race conditions, timing jitter, and CPU stalling during wireless operations, follow this partitioned ISR model:
1. **ISR Context (Polled SPI only)**:
   - Inside an interrupt handler (such as the `DIO1_IRQHandler` for the radio), only perform fast, polled (non-DMA) SPI transactions.
   - Limit transactions to reading and clearing interrupt flags (e.g., `GetIrqStatus` [0x12] and `ClearIrqStatus` [0x97] for SX1262), which takes $\approx 1\,\mu\text{s}$ at $8\,\text{MHz}$.
2. **Active Object Context (DMA SPI for payload)**:
   - Defer long SPI reads (e.g., `ReadBuffer` for the full packet payload) to the Active Object's execution context.
   - Use DMA SPI for payload reads. The buffer returned by `Frame::raw_buf_for_dma()` is 4-byte-aligned.
   - Block/wake using a QXK semaphore or QF event rather than spinning inside the ISR.

### NVIC Priority Configuration (Cortex-M / RISC-V CLIC)
To ensure the preemptive scheduler can lock critical sections without blocking high-priority interrupts:
- Any interrupt handler that calls `post_from_isr`, `qk::isr_entry`, or `qk::isr_exit` **MUST** be configured with a priority number **numerically $\ge$ `QK_BASEPRI`** (i.e., lower urgency).
- On Cortex-M (where lower numbers mean higher priority), if `QK_BASEPRI` is `0x50`, the RF DIO1 interrupt should be configured to `0xC0` (lower priority) so that writing to `BASEPRI` does not block it.

### SMP Kernel & Run-To-Completion (RTC) Preservation
When multicore execution is needed, configure the `smp` feature flag. This shifts `QvKernel` to an SMP-capable cooperative scheduler:
- **No Concurrency per AO**: Cores claim Active Objects using atomic Compare-and-Swap (CAS) on `executing_core` (CORE_ID_NONE/0xFF). This ensures that while multiple AOs run concurrently across cores, no single AO behavior ever executes concurrently or re-entrantly.
- **Run-To-Completion**: Event handlers run cooperatively on the dispatching core to completion. A core releases its ownership claim on the AO's slot only after `dispatch_one()` returns.
- **Core ID Resolution Mapping**: Resolved via `current_core_id()`. Standard hosted (`std`) test targets map OS threads dynamically to virtual core IDs (0..7) using thread-local storage, while `no_std` targets link to a hardware port hook `qf_port_current_core_id()`.

---

## 5. RF Protocol Stack (`comms`)

The modular RF stack decomposes communication into sequential protocol layers implementing the `Layer` trait:
- **`Frame`**: Packet buffer with support for prepending headers and stripping payloads. Supports 4-byte alignment for DMA/AES operations.
- **`FramePool`**: Static, no-allocation (`no_std`) packet pool.
- **`LoRaWanMac`**: Encapsulates cryptographic MIC computing, framing, and packet encryption.
- **`Network`**: Binds network ports to active object signals.
- **`ReliableTransport`**: Implements a TCP-like sliding-window (window size = 1, half-duplex) reliable transfer layer with automatic retry timeouts.
- **`RfStackAO`**: An active object state machine orchestrating the transition from `Idle` $\to$ `Transmitting` $\to$ `WaitingAck` $\to$ `Listening`.
- **`LoopbackPhy`**: A simulated PHY layer that loopbacks transmitted packets as received packets, enabling host-side integration testing.

---

## 6. Build, Test, and Simulation Workflows

### Cargo Commands
Standard cargo commands work out-of-the-box from the workspace root:
```bash
cargo check                 # Type check all crates
cargo build                 # Build workspace members (host default features)
cargo test                  # Run unit tests across crates
cargo test -p qf --features smp  # Run tests with SMP capability enabled
cargo build -p comms --no-default-features  # Verify no_std/alloc compliance
```

### Makefile Helper Targets
The root `Makefile` provides targets to check independent crates and build examples for target hardware:
```bash
make hal-check              # Check all independent HAL crates
make hal-check-cmsis        # Check hal-cmsis for stm32f4xx/nrf52840/lpc1768
make hal-check-rvsis        # Check hal-rvsis for esp32c6/esp32c3/gd32vf103
make hal-check-lxsis        # Check hal-lxsis for esp32/esp32s2/esp32s3

# Build/run examples (Boards: host, esp32s3, esp32c6; Examples: dpp, lora_send)
make example-host-dpp       # Build DPP example for host
make run-host-dpp           # Run DPP example on host
make example-esp32c6-dpp    # Build DPP for ESP32-C6
make flash-esp32c6-lora_send # Flash LoRa send example to ESP32-C6
```

### Renode Simulation
The repository features an automated simulation setup using [Renode](https://renode.io/) and the Robot Framework (configured in `configs/renode/`):
- **Battery Fault Injection**: Exercises battery and solar voltage monitors against fault profiles in virtual time.
- **LoRa Multi-node**: Tests wireless range and packet reception between multiple virtual nodes.

To run the simulation tests:
```bash
make renode-test-all        # Run all Robot Framework tests
make renode-battery-fault   # Run battery fault injection tests
make renode-lora-multinode  # Run LoRa range/multinode tests
```

---

## 7. Developer Rules (Zero Exceptions)

- **Do Not Mix Plaintext Logging and QS Tracing**: Never write to stdout/stderr or use plaintext loggers (`esp_println`, `defmt`) in a build where the `qs` feature is enabled. Plaintext characters corrupt the binary HDLC-framed QS stream. Plaintext logging must be gated behind `#[cfg(not(feature = "qs"))]`.
- **Match Driver Feature with Renode Model**: In multi-node simulation, a node's firmware driver feature (e.g. `lr1121` vs `sx1278`) **MUST** match the Renode radio C# model loaded in the `.resc` script. If they mismatch, the virtual SPI bus fails, the radio goes silent, and the simulation hangs.
- **Investigate Radio on "QS Silence"**: If a test starts, emits its QS dictionary records, and then hangs in silence, this almost always points to a broken radio link (feature mismatch or out of range) or a broken timing loop, not a tracing failure.
- **Keep Core Crates `no_std`**: `qf`, `qk`, `qxk`, `qs`, `comms`, and `hal` must compile without standard library or global allocator dependencies unless the `std` or `alloc` features are explicitly activated.
- **Commit Discipline**: Commit changes incrementally per logical step. Only include files directly related to the task; never stage unrelated build artifacts, lockfiles, or files under `target/`.
