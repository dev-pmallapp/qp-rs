# HAL Consolidation Plan: *SIS Family

Consolidate `hal-esp/`, `hal-ti/`, `hal-ht32/`, and `hal-cmsis/` into three
pure-Rust architecture ports — `hal-cmsis` (Cortex-M), `hal-lxsis` (Xtensa LX),
`hal-rvsis` (RISC-V) — sharing a single volatile register-access foundation in
`hal/src/mmio.rs`.

See `docs/src/cmsis.md`, `docs/src/lxsis.md`, `docs/src/rvsis.md` for the
architecture and code patterns each phase implements.

---

## Phase 1 — Foundation: `hal/src/mmio.rs`

**Blocks:** all other phases  
**Risk:** none

| Action | File |
|---|---|
| Create | `hal/src/mmio.rs` — `RW<T>`, `RO<T>`, `WO<T>` volatile wrappers |
| Modify | `hal/src/lib.rs` — add `pub mod mmio;` |

Verify: `cargo check -p hal`

---

## Phase 2 — Workspace scaffolding

**Blocks:** phases 3–5  
**Risk:** none

| Action | File |
|---|---|
| Modify | `hal/Cargo.toml` — add `hal-lxsis`, `hal-rvsis` to `[workspace] members` |
| Create | `hal/hal-lxsis/Cargo.toml` — dep `hal` only; features: `esp32`, `esp32s2`, `esp32s3`, `qp-integration` |
| Create | `hal/hal-lxsis/src/lib.rs` — `#![no_std]`, empty |
| Create | `hal/hal-rvsis/Cargo.toml` — dep `hal` only; features: `esp32c3`, `esp32c6`, `gd32vf103`, `qp-integration` |
| Create | `hal/hal-rvsis/src/lib.rs` — `#![no_std]`, empty |

Verify: `cargo check` in `hal/`

---

## Phase 3 — Implement `hal-cmsis/` (Cortex-M)

**Blocks:** Phase 7a  
**Risk:** low — pure Rust, no FFI, no build.rs

### Processor-core modules

| File | Content |
|---|---|
| `hal/hal-cmsis/src/asm.rs` | `dsb`, `isb`, `dmb`, `nop`, `wfi`, `wfe` via `core::arch::asm!` |
| `hal/hal-cmsis/src/basepri.rs` | `read() -> u8`, `unsafe write(u8)` — ARMv7-M only |
| `hal/hal-cmsis/src/nvic.rs` | `NvicRegs` struct + `NvicController : InterruptController` |
| `hal/hal-cmsis/src/systick.rs` | `SysTickRegs` struct + `SysTickTimer : Timer` |
| `hal/hal-cmsis/src/scb.rs` | `clean_dcache`, `invalidate_dcache` — stub bodies, M7 only |
| `hal/hal-cmsis/src/lib.rs` | update: add all `pub mod` declarations |

### STM32F4 vendor module — `#[cfg(feature = "stm32f4xx")]`

| File | Content |
|---|---|
| `hal/hal-cmsis/src/stm32f4/regs.rs` | `GpioRegs`, `SpiRegs`, `UsartRegs` structs + base address constants |
| `hal/hal-cmsis/src/stm32f4/gpio.rs` | `Stm32F4Pin : GpioPin` |
| `hal/hal-cmsis/src/stm32f4/spi.rs` | `Stm32F4Spi : SpiMaster` |
| `hal/hal-cmsis/src/stm32f4/uart.rs` | `Stm32F4Uart : Uart` |
| `hal/hal-cmsis/src/stm32f4/mod.rs` | re-exports |

### nRF52840 vendor module — `#[cfg(feature = "nrf52840")]`

| File | Content |
|---|---|
| `hal/hal-cmsis/src/nrf52/regs.rs` | `GpioRegs`, `SpiRegs`, `UarteRegs` + base addresses |
| `hal/hal-cmsis/src/nrf52/gpio.rs` | `Nrf52Pin : GpioPin` |
| `hal/hal-cmsis/src/nrf52/spi.rs` | `Nrf52Spi : SpiMaster` |
| `hal/hal-cmsis/src/nrf52/uart.rs` | `Nrf52Uart : Uart` |
| `hal/hal-cmsis/src/nrf52/mod.rs` | re-exports |

### `hal-cmsis/Cargo.toml` additions

Add `nrf52840` feature. Add placeholder features `cc26xx` and `ht32f5` (empty
modules — implementations added when those chips are needed).

### Retire `hal-ti/` and `hal-ht32/`

Both are empty stubs with no consumers.

| Action | Detail |
|---|---|
| Modify | `hal/Cargo.toml` — remove `hal-ti` and `hal-ht32` from members |
| Delete | `hal/hal-ti/` — entire directory |
| Delete | `hal/hal-ht32/` — entire directory |

Verify: `cargo check -p hal-cmsis --features stm32f4xx`

---

## Phase 4 — Implement `hal-lxsis/` (Xtensa LX)

**Blocks:** Phase 6 (ESP32 / ESP32-S3 migration)  
**Risk:** medium — Xtensa `RSR`/`WSR` inline asm; needs `xtensa-esp32-none-elf` target to verify

### Processor-core modules

| File | Content |
|---|---|
| `hal/hal-lxsis/src/asm.rs` | `memw`, `isync`, `nop`, `waiti` via `core::arch::asm!` |
| `hal/hal-lxsis/src/intlevel.rs` | `rsil(level) -> u32`, `unsafe wsr_ps(u32)` — interrupt level lock |
| `hal/hal-lxsis/src/intenable.rs` | `IntenableController : InterruptController` — `RSR`/`WSR` INTENABLE |
| `hal/hal-lxsis/src/ccompare.rs` | `CcompareTimer : Timer` — CCOUNT + CCOMPARE0 |
| `hal/hal-lxsis/src/lib.rs` | update: add all `pub mod` declarations |

### ESP32 vendor module — `#[cfg(feature = "esp32")]`

| File | Content |
|---|---|
| `hal/hal-lxsis/src/esp32/regs.rs` | `GpioRegs`, `SpiRegs`, `UartRegs` + base addresses (from ESP32 TRM) |
| `hal/hal-lxsis/src/esp32/gpio.rs` | `Esp32Pin : GpioPin` |
| `hal/hal-lxsis/src/esp32/spi.rs` | `Esp32Spi : SpiMaster` |
| `hal/hal-lxsis/src/esp32/uart.rs` | `Esp32Uart : Uart` |
| `hal/hal-lxsis/src/esp32/mod.rs` | re-exports |

### ESP32-S3 vendor module — `#[cfg(feature = "esp32s3")]`

Same peripheral layout as ESP32, different base addresses (LX7 core).

| File | Content |
|---|---|
| `hal/hal-lxsis/src/esp32s3/regs.rs` | ESP32-S3 base addresses + register structs |
| `hal/hal-lxsis/src/esp32s3/gpio.rs` | `Esp32S3Pin : GpioPin` |
| `hal/hal-lxsis/src/esp32s3/spi.rs` | `Esp32S3Spi : SpiMaster` |
| `hal/hal-lxsis/src/esp32s3/uart.rs` | `Esp32S3Uart : Uart` |
| `hal/hal-lxsis/src/esp32s3/mod.rs` | re-exports |

Verify: `cargo check -p hal-lxsis --features esp32 --target xtensa-esp32-none-elf`

---

## Phase 5 — Implement `hal-rvsis/` (RISC-V)

**Blocks:** Phase 6 (ESP32-C6 migration)  
**Risk:** medium-low — standard RISC-V CSR asm well-supported; ESP32-C3/C6 interrupt matrix is chip-specific

### Processor-core modules

| File | Content |
|---|---|
| `hal/hal-rvsis/src/asm.rs` | `fence`, `fence_i`, `nop`, `wfi` via `core::arch::asm!` |
| `hal/hal-rvsis/src/csr.rs` | `csrr`, `csrw`, `csrrs`, `csrrc` generic helpers |
| `hal/hal-rvsis/src/mstatus.rs` | `qk_lock() -> u32`, `qk_unlock(u32)` — clear/restore `MIE` bit |
| `hal/hal-rvsis/src/plic.rs` | `PlicController : InterruptController` — standard PLIC layout at configurable base |
| `hal/hal-rvsis/src/clint.rs` | `ClintTimer : Timer` — `mtime`/`mtimecmp` at configurable base |
| `hal/hal-rvsis/src/lib.rs` | update: add all `pub mod` declarations |

### ESP32-C6 vendor module — `#[cfg(feature = "esp32c6")]`

This is the first module that must work end-to-end (drives SX1262 in `lora_send`).

| File | Content |
|---|---|
| `hal/hal-rvsis/src/esp32c6/regs.rs` | GPIO, SPI, UART register structs (from ESP32-C6 TRM) |
| `hal/hal-rvsis/src/esp32c6/gpio.rs` | `Esp32C6Pin : GpioPin` |
| `hal/hal-rvsis/src/esp32c6/spi.rs` | `Esp32C6Spi : SpiMaster` |
| `hal/hal-rvsis/src/esp32c6/uart.rs` | `Esp32C6Uart : Uart` |
| `hal/hal-rvsis/src/esp32c6/intmtx.rs` | `Esp32C6IntMatrix : InterruptController` — ESP32-C6 interrupt routing |
| `hal/hal-rvsis/src/esp32c6/mod.rs` | re-exports |

### ESP32-C3 vendor module — `#[cfg(feature = "esp32c3")]`

| File | Content |
|---|---|
| `hal/hal-rvsis/src/esp32c3/regs.rs` | ESP32-C3 peripheral register structs |
| `hal/hal-rvsis/src/esp32c3/gpio.rs` | `Esp32C3Pin : GpioPin` |
| `hal/hal-rvsis/src/esp32c3/spi.rs` | `Esp32C3Spi : SpiMaster` |
| `hal/hal-rvsis/src/esp32c3/uart.rs` | `Esp32C3Uart : Uart` |
| `hal/hal-rvsis/src/esp32c3/intmtx.rs` | `Esp32C3IntMatrix : InterruptController` |
| `hal/hal-rvsis/src/esp32c3/mod.rs` | re-exports |

### GD32VF103 vendor module — `#[cfg(feature = "gd32vf103")]`

Uses standard `plic.rs` and `clint.rs` — only peripheral (GPIO/SPI/UART) register structs needed.

| File | Content |
|---|---|
| `hal/hal-rvsis/src/gd32vf/regs.rs` | GD32VF103 peripheral register structs |
| `hal/hal-rvsis/src/gd32vf/gpio.rs` | `Gd32VfPin : GpioPin` |
| `hal/hal-rvsis/src/gd32vf/spi.rs` | `Gd32VfSpi : SpiMaster` |
| `hal/hal-rvsis/src/gd32vf/uart.rs` | `Gd32VfUart : Uart` |
| `hal/hal-rvsis/src/gd32vf/mod.rs` | re-exports |

Verify: `cargo check -p hal-rvsis --features esp32c6 --target riscv32imac-unknown-none-elf`

---

## Phase 6 — Migrate `hal-esp/`

`hal-esp/` is an FFI wrapper around `esp-idf-sys` (C bindings to the ESP-IDF SDK),
not pure Rust. Migration is a two-step: port the radio drivers first, then delete
`hal-esp/` once the single consumer is updated.

### Step 6a — Port radio drivers into new crates

The radio driver logic (SX1262 / SX1276 register sequences) is
chip-independent. Copy the bodies, swap the SPI type.

| Source | Destination |
|---|---|
| `hal/hal-esp/src/sx1262.rs` | `hal/hal-rvsis/src/esp32c6/radio/sx1262.rs` — uses `Esp32C6Spi` |
| `hal/hal-esp/src/sx1276.rs` | `hal/hal-lxsis/src/esp32s3/radio/sx1276.rs` — uses `Esp32S3Spi` |

Both destinations implement the existing `hal::lora::RfDriver` trait unchanged.

### Step 6b — Update the sole consumer

`examples/lora_send/src/bin/esp32_c6.rs` is the only file in the main
workspace that depends on `hal-esp`.

| Action | Detail |
|---|---|
| Modify | `examples/lora_send/Cargo.toml` — replace `hal-esp` dep with `hal-rvsis` |
| Modify | `examples/lora_send/src/bin/esp32_c6.rs` — replace `EspSpiMaster`/`EspGpioPin` with `Esp32C6Spi`/`Esp32C6Pin`; replace `hal_esp::Sx1262` with `hal_rvsis::esp32c6::radio::Sx1262` |

Verify: `cargo build --bin lora_send_c6 --features esp32c6 --no-default-features`

### Step 6c — Delete `hal-esp/`

| Action | Detail |
|---|---|
| Modify | `hal/Cargo.toml` — remove `hal-esp` from workspace members |
| Delete | `hal/hal-esp/` — entire directory |

---

## Phase 7 — Upgrade ports

### 7a — `ports/cortex-m/`

Switch from the `cortex-m` crate to `hal-cmsis` for BASEPRI and barrier
instructions. The `cortex-m` crate dependency is removed entirely.

| Action | Detail |
|---|---|
| Modify | `ports/cortex-m/Cargo.toml` — add `hal-cmsis`; remove `cortex-m` |
| Modify | `ports/cortex-m/src/nvic_cfg.rs` — `use hal_cmsis::basepri` / `use hal_cmsis::asm` |
| Modify | `ports/cortex-m/src/lib.rs` — replace `cortex_m::asm::dsb()` etc. |

### 7b — New `ports/xtensa/`

| File | Content |
|---|---|
| `ports/xtensa/Cargo.toml` | deps: `hal-lxsis`, `qk`, `qxk` |
| `ports/xtensa/src/lib.rs` | interrupt dispatch; WindowOverflow/Underflow handlers |
| `ports/xtensa/src/context.rs` | `ContextFrame` — Xtensa windowed register spill layout |
| `ports/xtensa/src/intlevel_cfg.rs` | `QK_INTLEVEL` const; `qk_lock`/`qk_unlock` via `hal_lxsis::intlevel` |

### 7c — New `ports/riscv/`

| File | Content |
|---|---|
| `ports/riscv/Cargo.toml` | deps: `hal-rvsis`, `qk`, `qxk` |
| `ports/riscv/src/lib.rs` | machine-mode trap handler; software interrupt for context switch |
| `ports/riscv/src/context.rs` | `ContextFrame` — RISC-V caller-saved register layout (per ABI) |
| `ports/riscv/src/mstatus_cfg.rs` | `qk_lock`/`qk_unlock` via `hal_rvsis::mstatus` |

---

## Phase dependency graph

```
Phase 1 (mmio)
    │
Phase 2 (workspace scaffold)
    ├────────────────┬─────────────────┐
Phase 3 (hal-cmsis) Phase 4 (hal-lxsis) Phase 5 (hal-rvsis)
    │                    │                   │
Phase 7a                 │              Phase 6 (migrate hal-esp)
(ports/cortex-m)         │                   │
                         └──────────┬─────────┘
                              Phase 7b/c
                          (ports/xtensa, ports/riscv)
```

Phases 3, 4, 5 are independent of each other and can be worked in parallel.

---

## What is NOT in this plan

- `ports/esp32-s3/` and `ports/esp32-c6/` — updated separately to depend on
  `ports/xtensa` and `ports/riscv` respectively once Phase 7b/c lands
- `RfPhy` trait (`RF_STACK_PLAN.md`) — independent; radio drivers in Phase 6a
  implement the existing `RfDriver` trait unchanged
- `hal/src/mmio.rs` as the single volatile wrapper source means all future
  peripheral additions automatically use it — no per-phase maintenance
