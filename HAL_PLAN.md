# HAL Consolidation Plan: *SIS Family

Consolidate `hal-esp/`, `hal-ti/`, `hal-ht32/`, and `hal-cmsis/` into three
pure-Rust architecture ports — `hal-cmsis` (Cortex-M), `hal-lxsis` (Xtensa LX),
`hal-rvsis` (RISC-V) — sharing a single volatile register-access foundation in
`hal/src/mmio.rs`.  A fourth crate `hal-esp-idf` provides the ESP-IDF C SDK
wrapper path for WiFi/BLE/OTA use cases.

See `docs/src/cmsis.md`, `docs/src/lxsis.md`, `docs/src/rvsis.md` for the
architecture and code patterns.

**Legend:** ✅ Complete · 🔲 Not started · ⚠️ Partial / gap

---

## Phase 1 — Foundation: `hal/src/mmio.rs` ✅

| Action | File | Status |
|---|---|---|
| Create | `hal/src/mmio.rs` — `RW<T>`, `RO<T>`, `WO<T>` volatile wrappers | ✅ |
| Modify | `hal/src/lib.rs` — add `pub mod mmio;` | ✅ |

---

## Phase 2 — Workspace scaffolding ✅

| Action | File | Status |
|---|---|---|
| Modify | `hal/Cargo.toml` — add `hal-lxsis`, `hal-rvsis`; comment out `hal-esp` | ✅ |
| Create | `hal/hal-lxsis/Cargo.toml` + `src/lib.rs` | ✅ |
| Create | `hal/hal-rvsis/Cargo.toml` + `src/lib.rs` | ✅ |
| Remove | `hal/hal-ti/` and `hal/hal-ht32/` from workspace + delete directories | ✅ |

---

## Phase 3 — Implement `hal-cmsis/` (Cortex-M) ✅

### Processor-core modules

| File | Status |
|---|---|
| `hal/hal-cmsis/src/asm.rs` | ✅ |
| `hal/hal-cmsis/src/basepri.rs` | ✅ |
| `hal/hal-cmsis/src/nvic.rs` | ✅ |
| `hal/hal-cmsis/src/systick.rs` | ✅ |
| `hal/hal-cmsis/src/scb.rs` | ✅ |
| `hal/hal-cmsis/src/lib.rs` — all `pub mod` declarations | ✅ |

### STM32F4 vendor module

| File | Status |
|---|---|
| `hal/hal-cmsis/src/stm32f4/regs.rs` | ✅ |
| `hal/hal-cmsis/src/stm32f4/gpio.rs` | ✅ |
| `hal/hal-cmsis/src/stm32f4/spi.rs` | ✅ |
| `hal/hal-cmsis/src/stm32f4/uart.rs` | ✅ |
| `hal/hal-cmsis/src/stm32f4/mod.rs` | ✅ |

### nRF52840 vendor module

| File | Status |
|---|---|
| `hal/hal-cmsis/src/nrf52/regs.rs` | ✅ |
| `hal/hal-cmsis/src/nrf52/gpio.rs` | ✅ |
| `hal/hal-cmsis/src/nrf52/spi.rs` | ✅ |
| `hal/hal-cmsis/src/nrf52/uart.rs` | ✅ |
| `hal/hal-cmsis/src/nrf52/mod.rs` | ✅ |

---

## Phase 4 — Implement `hal-lxsis/` (Xtensa LX) ✅

### Processor-core modules

| File | Status |
|---|---|
| `hal/hal-lxsis/src/asm.rs` | ✅ |
| `hal/hal-lxsis/src/intlevel.rs` | ✅ |
| `hal/hal-lxsis/src/intenable.rs` | ✅ |
| `hal/hal-lxsis/src/ccompare.rs` | ✅ |

### ESP32 vendor module

| File | Status |
|---|---|
| `hal/hal-lxsis/src/esp32/regs.rs` | ✅ |
| `hal/hal-lxsis/src/esp32/gpio.rs` | ✅ |
| `hal/hal-lxsis/src/esp32/spi.rs` | ✅ |
| `hal/hal-lxsis/src/esp32/uart.rs` | ✅ |
| `hal/hal-lxsis/src/esp32/mod.rs` | ✅ |

### ESP32-S3 vendor module + radio

| File | Status |
|---|---|
| `hal/hal-lxsis/src/esp32s3/regs.rs` | ✅ |
| `hal/hal-lxsis/src/esp32s3/gpio.rs` | ✅ |
| `hal/hal-lxsis/src/esp32s3/spi.rs` | ✅ |
| `hal/hal-lxsis/src/esp32s3/uart.rs` | ✅ |
| `hal/hal-lxsis/src/esp32s3/mod.rs` | ✅ |
| `hal/hal-lxsis/src/esp32s3/radio/sx1276.rs` | ✅ (ahead of plan) |

### ESP32-S2 vendor module ⚠️

| File | Status |
|---|---|
| `hal/hal-lxsis/src/esp32s2/` | ⚠️ Empty placeholder — shares peripheral layout with ESP32, different base addresses |

---

## Phase 5 — Implement `hal-rvsis/` (RISC-V) ✅

### Processor-core modules

| File | Status |
|---|---|
| `hal/hal-rvsis/src/asm.rs` | ✅ |
| `hal/hal-rvsis/src/csr.rs` | ✅ |
| `hal/hal-rvsis/src/mstatus.rs` | ✅ |
| `hal/hal-rvsis/src/plic.rs` | ✅ |
| `hal/hal-rvsis/src/clint.rs` | ✅ |

### ESP32-C6 vendor module + radio

| File | Status |
|---|---|
| `hal/hal-rvsis/src/esp32c6/regs.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c6/gpio.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c6/spi.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c6/uart.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c6/intmtx.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c6/mod.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c6/radio/sx1262.rs` | ✅ (ahead of plan) |

### ESP32-C3 vendor module

| File | Status |
|---|---|
| `hal/hal-rvsis/src/esp32c3/regs.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c3/gpio.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c3/spi.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c3/uart.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c3/intmtx.rs` | ✅ |
| `hal/hal-rvsis/src/esp32c3/mod.rs` | ✅ |

### GD32VF103 vendor module

| File | Status |
|---|---|
| `hal/hal-rvsis/src/gd32vf/regs.rs` | ✅ |
| `hal/hal-rvsis/src/gd32vf/gpio.rs` | ✅ |
| `hal/hal-rvsis/src/gd32vf/spi.rs` | ✅ |
| `hal/hal-rvsis/src/gd32vf/uart.rs` | ✅ |
| `hal/hal-rvsis/src/gd32vf/mod.rs` | ✅ |

---

## Phase 6 — Migrate `hal-esp/` ✅ + Create `hal-esp-idf/` ✅

The original plan called for deleting `hal-esp/` once consumers migrated.
`hal-esp/` is gone. `hal-esp-idf/` has been created as the ESP-IDF C SDK
wrapper path — separate from and coexisting with the pure-Rust `hal-lxsis`
and `hal-rvsis`.

### Coexistence model

```
ESP32-C6 binary
    ├── hal-rvsis (pure Rust: GPIO, SPI, UART, timer, interrupts)
    └── hal-esp-idf (ESP-IDF C SDK: WiFi, BLE, NVS, OTA, TLS)
        └── esp-idf-sys (C bindings — build.rs + embuild)
```

Both implement the same `hal/src/` traits.  `esp-idf-sys` `binstart` provides
the C runtime init without forcing all peripheral access through C APIs.

| Action | Status |
|---|---|
| Port `sx1262.rs` to `hal-rvsis/src/esp32c6/radio/` | ✅ |
| Port `sx1276.rs` to `hal-lxsis/src/esp32s3/radio/` | ✅ |
| Update `lora_send` to use `hal-rvsis` | ✅ |
| Delete `hal-esp/` directory | ✅ |
| Create `hal-esp-idf/` with GPIO/SPI/UART wrappers | ✅ |
| Add `hal-esp-idf` to workspace (commented out) | ✅ |

---

## Phase 7 — Upgrade ports ✅

| Port | Action | Status |
|---|---|---|
| `ports/cortex-m` | Add `hal-cmsis` dep; remove `cortex-m` crate | ✅ |
| `ports/xtensa` | Create with `hal-lxsis`, QXK context frame, intlevel_cfg | ✅ |
| `ports/riscv` | Create with `hal-rvsis`, QXK context frame, mstatus_cfg | ✅ |

Device-specific ports (`ports/esp32-c6`, `ports/esp32-s3`) exist but are
handled separately — see gap below.

---

## Gaps and improvements

### ✅ G1 — `esp32s2` module implemented

`hal/hal-lxsis/src/esp32s2/` now contains `regs.rs`, `gpio.rs`, `spi.rs`,
`uart.rs`, and `mod.rs`.  Register struct layouts mirror ESP32-S3; the GPIO
base address is 0x6004\_4000 (distinct from S3's 0x6000\_4000).  `lib.rs`
promotes the inline `pub mod esp32s2 {}` placeholder to `pub mod esp32s2;`.

### ✅ G2 — `ports/esp32-c6` and `ports/esp32-s3` now depend on HAL crates

Both `Cargo.toml` files gain `hal` and `hal-rvsis`/`hal-lxsis` as optional
deps, activated by the existing `rt` feature.  `configure_priorities()` calls
into `PlicController` (C6) / `IntenableController` (S3), and
`configure_periodic()` calls into `ClintTimer` (C6) / `CcompareTimer` (S3).
Non-`rt` tests continue to pass unchanged.

### ✅ G3 — Makefile HAL smoke-build targets added

`Makefile` gains `hal-check`, `hal-check-cmsis`, `hal-check-lxsis`, and
`hal-check-rvsis` targets.  Each runs `cargo check` in the `hal/` sub-workspace
with the relevant chip features, covering all advertised vendor modules.

### ✅ G4 — `hal-cmsis` `lpc1768` vendor module implemented

`hal/hal-cmsis/src/lpc17/` now contains `regs.rs` (Fast GPIO, SSP, UART
register maps at 0x2009\_C000 / 0x4008\_8000 / 0x4000\_C000), `gpio.rs`,
`spi.rs`, `uart.rs`, and `mod.rs`.  `lib.rs` gates the module on the existing
`lpc1768` feature.

### ✅ G5 — `hal-esp-idf` `configure()` on `EspSpiMaster` implemented

`configure()` now calls `spi_bus_remove_device` + `spi_bus_add_device` with the
new `SpiConfig`, enabling correct SPI reconfiguration on multi-device shared
buses.

---

## Updated dependency graph

```
Phase 1 (mmio) ✅
    │
Phase 2 (workspace) ✅
    ├──────────────────┬──────────────────┐
Phase 3 (hal-cmsis) ✅  Phase 4 (hal-lxsis) ✅  Phase 5 (hal-rvsis) ✅
    │                      │                      │
Phase 7a ✅                Phase 7b ✅             Phase 6 ✅ + Phase 7c ✅
(ports/cortex-m)          (ports/xtensa)          (hal-esp-idf + ports/riscv)

All gaps resolved ✅
```
