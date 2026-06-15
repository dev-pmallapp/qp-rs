# SWM Renode Simulation Workspace

This directory contains the [Renode](https://renode.io/) simulation infrastructure for the SWM
firmware workspace.  It provides cycle-accurate emulation of the ESP32-C6 (RISC-V RV32IMAC) and
STM32 Cortex-M nodes (OHT, SPT, MC roles), a four-machine LoRa multi-node topology for RF
communication testing, and an automated Robot Framework regression suite covering battery fault
injection, QS actor-tracing, and wireless range enforcement — all running against deterministic
virtual time so results are reproducible without physical hardware.

---

## Directory Structure

```
configs/renode/
│
├── isa/                              # Layer 0 — ISA templates (documentation only)
│   ├── riscv/
│   │   └── _template.repl           # Bare RISC-V CPU + CLINT + PLIC stubs
│   └── arm/
│       └── _template.repl           # Bare Cortex-M CPU + NVIC stubs
│
├── soc/                              # Layer 1 — SoC platform descriptions
│   ├── riscv/
│   │   ├── esp32c6/                  # ESP32-C6 (RV32IMAC)
│   │   │   └── esp32c6.repl          # Memory map, CPU, UART, SPI, GPIO, ADC
│   │   └── roms/                     # ESP32-Cx ROM ELF blobs for boot simulation
│   │
│   └── arm-cortex-m/
│       ├── stm32f4/                  # STM32F4xx (Cortex-M4F)
│       └── stm32g0b1/                # STM32G0B1 (Cortex-M0+)
│
├── platform/                         # Layer 2a — Vendor devkit launch scripts
│   ├── riscv/
│   │   ├── esp32c6_devkitc/
│   │   │   └── esp32c6_devkitc.resc  # ESP32-C6 DevKit-C interactive launch
│   │   ├── esp32c6_lr1121/
│   │   │   └── esp32c6_lr1121.resc   # ESP32-C6 DevKit + LR1121 LoRa
│   │   └── esp32c6_sx1278/
│   │       └── esp32c6_sx1278.resc   # ESP32-C6 DevKit + SX1278 LoRa
│   └── arm/
│       └── stm32g0b1_devkit/
│           └── stm32g0b1_devkit.resc # STM32G0B1 devkit launch script
│
├── swm/                              # SWM-specific multi-node simulation
│   ├── ref_platform/
│   │   ├── node_sx1276.repl          # STM32F4 + SX1276 sensor/gateway node
│   │   └── node_sx1262.repl          # STM32F4 + SX1262 sensor/gateway node
│   └── README.md                     # LoRa multi-node layout and quick-start
│
├── scripts/                          # Renode session scripts (.resc)
│   ├── lora_multinode.resc           # Four-machine LoRa topology (sensors + gateway)
│   └── debug_session.resc            # Interactive debug helper
│
├── tests/                            # Robot Framework test suites
│   ├── battery_fault_test.robot      # Battery fault-injection regression (primary suite)
│   ├── test_lora_multinode.robot     # LoRa multi-node TX/RX and range tests
│   ├── test_battery_fault.resc       # Standalone Renode fault-injection script
│   ├── step1_connection.robot        # Smoke: Renode monitor responds
│   ├── step2_platform_load.robot     # Smoke: ESP32-C6 platform loads cleanly
│   ├── step3_teardown.robot          # Smoke: load / clear / reload cycle
│   └── step4_runfor_hooks.robot      # Smoke: RunFor + symbol hooks execute
│
├── shared/                           # Reusable building blocks
│   ├── peripherals/
│   │   ├── battery/
│   │   │   └── battery_adc_stub.py   # ESP32-C6 ADC stub: battery + solar voltage injection
│   │   ├── lora/
│   │   │   ├── lora.cs               # C# LoRa peripheral base class
│   │   │   ├── sx1278_spi.py         # SX1278 SPI transceiver emulation (GPSPI2)
│   │   │   └── lr1121_radio.cs       # LR1121 SPI + IRadio transceiver (GPSPI2 + wireless medium)
│   │   ├── console/
│   │   │   └── usb_serial_jtag.cs    # ESP32-C6 USB Serial/JTAG console model
│   │   ├── sensor/
│   │   │   └── ultrasonic_hcsr04.py  # HC-SR04 ultrasonic sensor via GPIO
│   │   ├── timer/
│   │   │   ├── systimer_stub.py      # ESP32-C6 SYSTIMER (16 MHz, snapshot protocol)
│   │   │   └── timg_stub.py          # ESP32-C6 TIMG timer group stub
│   │   ├── renode_swm/               # Python helper package used by all stubs
│   │   │   ├── __init__.py
│   │   │   └── common.py             # PReg, mock classes for IDE linting
│   │   ├── renode_linter_helper.py   # .resc linter utility
│   │   └── generic_uart.repl         # Fallback UART stub
│   │
│   ├── macros/
│   │   ├── common.resc               # status, reset, verbose, quiet, hexdump, snapshot macros
│   │   └── helpers.resc              # peripheral dump / soft-reset helpers
│   │
│   └── robot-keywords/
│       └── common_keywords.robot     # Shared RF keywords: Load Platform, Wait For Boot, etc.
│
├── tools/
│   └── run.sh                        # CLI wrapper: list / run / test / gdb
│
├── docs/                             # Reserved for simulation notes and errata
└── logs/                             # Runtime output (PCAP dumps, UART logs — git-ignored)
```

---

## Quick Start

### 1. Install Renode (>= 1.14)

```bash
wget https://builds.renode.io/renode-latest.linux-portable-dotnet.tar.gz
tar xf renode-latest.linux-portable-dotnet.tar.gz
export PATH=$PATH:$(pwd)/renode_portable
```

### 2. Build the firmware

For ESP32-C6 with QS tracing and the LR1121 LoRa driver (required by the battery fault suite):

```bash
cargo build --features qs,lr1121
```

Or via Make (builds the ESP firmware with QS tracing, then launches Renode interactively):

```bash
make renode                          # ESP32-C6 (default)
make renode TARGET=stm32g0b1         # STM32G0B1 Cortex-M node
```

### 3. Run the automated Robot Framework tests

Primary battery fault-injection suite:

```bash
make renode-battery-fault
# Results: target/test-results/battery_fault_test/{log.html,report.html,output.xml}
```

LoRa multi-node communication suite:

```bash
make renode-lora-multinode
```

Smoke tests (platform load, teardown, RunFor hooks):

```bash
make renode-step1
make renode-step2
make renode-step3
make renode-step4
```

All suites in one pass (continues on failure):

```bash
make renode-test-all
```

Or run manually with explicit output paths:

```bash
renode-test \
  --outputdir target/test-results/battery_fault_test \
  --output    target/test-results/battery_fault_test/output.xml \
  --log       target/test-results/battery_fault_test/log.html \
  --report    target/test-results/battery_fault_test/report.html \
  configs/renode/tests/battery_fault_test.robot \
  --variable FIRMWARE_PATH:target/riscv32imac-unknown-none-elf/debug/swm-gagan-esp32c6
```

### 4. Launch an interactive simulation

```bash
# ESP32-C6 with LR1121 (headless)
renode --disable-xwt configs/renode/platform/riscv/esp32c6_lr1121/esp32c6_lr1121.resc

# Four-node LoRa topology
renode --disable-xwt configs/renode/scripts/lora_multinode.resc

# Via the tools wrapper
./configs/renode/tools/run.sh run riscv esp32c6
./configs/renode/tools/run.sh list
```

---

## Test Suites

### Battery Fault Injection (`battery_fault_test.robot`)

Exercises the `BatteryManagerAO` and `AppCoordinatorAO` active objects against
injected battery voltages, all in deterministic virtual time.

| Test case | Fault injected | Assertion |
|---|---|---|
| Normal Boot Emits QS Frames | none | at least one QS frame within 500 ms virtual time |
| Battery Hard Fault | 1000 mV (below 2400 mV floor in `battery.rs`) | FSM enters Fault; QS frames continue; no abort |
| Battery Low SoC | 2500 mV (low %, triggers `coordinator.rs:577`) | AppCoordinatorAO enters BatteryLow path; no abort |
| Battery Restored After Fault | fault then restore to 3700 mV | QS frames resume within 500 ms after restore |

The suite hooks `EspQsSink::emit_frame` (resolved by `nm` at test time) and
`_default_abort` into scratchpad memory at `0x60002FF0` / `0x60002FF4` to avoid
a `monitor.Execute` round-trip during `RunFor`.

### LoRa Multi-Node Communication (`test_lora_multinode.robot`)

Four simulated nodes connected via a shared `IEEE802_15_4Medium` with a 100-unit
range limit:

```
(0,0,0)           (25,0,0)          (50,0,0)         (200,0,0)
node-sensor-1   node-gateway     node-sensor-2   node-out-of-range
  [SX1276]         [SX1262]          [SX1276]          [SX1276]
```

| ID | Test case | What it validates |
|---|---|---|
| TC-1 | All nodes boot | Radio init logged on UART for every node |
| TC-2 | Sensor 1 to gateway | Basic TX/RX within range (distance 25) |
| TC-3 | Sensor 2 to gateway | Basic TX/RX within range (distance 25) |
| TC-4 | Gateway broadcast | Single TX reaches both in-range sensors |
| TC-5 | Out-of-range isolation | No reception beyond 100-unit range |
| TC-6 | Dynamic disconnect / reconnect | Hot-unplug; traffic stops then resumes |
| TC-7 | Multi-hop routing | Gateway relays sensor-1 packet to sensor-2 |

### Smoke Tests (`step1` – `step4`)

Lightweight sanity checks run without application firmware: Renode monitor
connectivity, ESP32-C6 LR1121 platform load / clear / reload cycle, and
`RunFor` with symbol hooks.

---

## Peripheral Stubs

| Stub | File | What it models |
|---|---|---|
| Battery / Solar ADC | `battery/battery_adc_stub.py` | ESP32-C6 ADC1/2 (`0x6000E000`); SAR data registers + magic write registers for voltage injection at `+0xFF0` (battery mV), `+0xFF4` (solar mV), `+0xFF8` (charge-stat pin) |
| SX1278 LoRa SPI | `lora/sx1278_spi.py` | Semtech SX1278 over ESP32-C6 GPSPI2; register read/write via buffer registers, USR bit triggers transfer |
| LR1121 LoRa | `lora/lr1121_radio.cs` | Semtech LR1121 over ESP32-C6 GPSPI2; LR1121 command opcode set + IRadio wireless-medium peer for multinode simulation |
| USB Serial/JTAG | `console/usb_serial_jtag.cs` | ESP32-C6 USB console for QS frame and log output |
| Ultrasonic HC-SR04 | `sensor/ultrasonic_hcsr04.py` | GPIO trigger/echo protocol (GPIO5/6/7); echo pulse width maps to tank distance |
| SYSTIMER | `timer/systimer_stub.py` | ESP32-C6 SYSTIMER at 16 MHz; snapshot protocol advances 100 µs per trigger |
| TIMG timer group | `timer/timg_stub.py` | ESP32-C6 TIMG timer group stub |
| Generic UART | `generic_uart.repl` | Fallback UART stub when the native peripheral class is absent |

The `renode_swm` Python package (`shared/peripherals/renode_swm/`) provides
the `PReg` register-helper class and linter mocks used by all Python stubs.

---

## Shared Macros

`shared/macros/common.resc` — load from any `.resc` with
`include @shared/macros/common.resc`.  
Provides: `$status`, `$reset`, `$verbose`, `$quiet`, `$hexdump`,
`$save_snapshot`, `$load_snapshot`.

`shared/macros/helpers.resc` — peripheral dump and soft-reset helpers.

`shared/robot-keywords/common_keywords.robot` — import in any `.robot` file:

```robot
Resource    ${CURDIR}/../../shared/robot-keywords/common_keywords.robot
```

Keywords: `Create Platform`, `Load Firmware ELF`, `Wait For Boot String`,
`Memory Region Should Be Accessible`, `Start GDB Server`, `Advance Time By`.

---

## GDB Port Convention

| Architecture | Default port |
|---|---|
| RISC-V (ESP32-C6) | 3333 |
| ARM Cortex-M (STM32) | 3334 |

Attach with the tools wrapper:

```bash
# Terminal 1 — start simulation
./configs/renode/tools/run.sh run riscv esp32c6

# Terminal 2 — attach GDB
./configs/renode/tools/run.sh gdb riscv esp32c6 3333
```

---

## Further Reading

- [`docs/05-verification/simulation.md`](../../docs/05-verification/simulation.md) — simulation architecture, peripheral model design, QS tracing over USB Serial/JTAG
- [`docs/05-verification/TestingTopics.md`](../../docs/05-verification/TestingTopics.md) — full catalog of testing topics by area (peripheral unit tests, platform tests, LoRa communication, actor FSM, fault injection, CI/CD)
