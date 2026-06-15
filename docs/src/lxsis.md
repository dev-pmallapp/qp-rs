# Pure Rust Xtensa LX Port (LXSIS)

This document covers `hal-lxsis` — the Xtensa LX member of the `*SIS` port
family.  For the cross-crate pattern shared by all three ports see
[CMSIS Port §2](./cmsis.md#2-the-sis-cross-crate-pattern).

Supported targets: **ESP32** (Xtensa LX6), **ESP32-S2** (Xtensa LX7),
**ESP32-S3** (Xtensa LX7 dual-core).

---

## 1. Xtensa LX vs Cortex-M — key differences

| Aspect | Cortex-M (CMSIS) | Xtensa LX (LXSIS) |
|---|---|---|
| Interrupt controller | NVIC — memory-mapped registers | Special registers: `INTENABLE`, `INTERRUPT`, `INTSET`, `INTCLEAR` accessed via `RSR`/`WSR` instructions |
| Tick timer | SysTick — memory-mapped, single | `CCOUNT` cycle counter + `CCOMPARE0/1/2` — CPU special registers |
| Scheduler lock | `BASEPRI` register (M3/M4/M7) | `PS.INTLEVEL` field — set via `RSIL` / `WSR.PS` |
| Memory barrier | `dsb` / `dmb` | `memw` (memory wait) |
| Instruction sync | `isb` | `isync` |
| Peripheral registers | Memory-mapped, `RW<T>` | Memory-mapped, same `hal/src/mmio.rs` `RW<T>` |
| Critical section | `cpsid i` / `PRIMASK` | `RSIL a, N` (raise interrupt level to N) |

The key structural difference: interrupt control and the timer on Xtensa LX
are **CPU special registers** read/written with `RSR`/`WSR` instructions,
not memory-mapped peripheral registers.  GPIO, SPI, UART, DMA, and all other
peripherals are still memory-mapped and use `hal/src/mmio.rs` exactly as in
`hal-cmsis`.

---

## 2. Repository layout

```
hal/
  src/
    mmio.rs          ← shared with hal-cmsis and hal-rvsis
    gpio.rs, spi.rs, uart.rs, timer.rs, interrupt.rs, error.rs

  hal-lxsis/
    Cargo.toml       feature per chip variant; dep on hal only
    src/
      lib.rs         re-exports; #![no_std]
      asm.rs         memw / isync / nop / wfi / wfe
      intlevel.rs    rsil / wsr_ps — interrupt level lock
      intenable.rs   IntenableController : InterruptController
      ccompare.rs    CcompareTimer : Timer
      esp32/         #[cfg(feature = "esp32")]
        mod.rs
        regs.rs      GPIO / SPI / UART register structs
        gpio.rs      Esp32Pin : GpioPin
        spi.rs       Esp32Spi : SpiMaster
        uart.rs      Esp32Uart : Uart
      esp32s3/       #[cfg(feature = "esp32s3")]
        …            (LX7 — same peripheral layout, different base addresses)

ports/
  xtensa/            ← QXK scheduler glue for Xtensa LX
    src/
      lib.rs         interrupt dispatch; WindowOverflow/WindowUnderflow handlers
      context.rs     ContextFrame — Xtensa windowed register spill layout
      intlevel_cfg.rs QK_INTLEVEL constant + qk_lock / qk_unlock
```

---

## 3. Special register access — `hal-lxsis/src/asm.rs`

Xtensa LX uses the `RSR` (Read Special Register) and `WSR` (Write Special
Register) instructions for CPU-level registers.  The register name is
encoded as an immediate operand, not a memory address.

```rust
// hal/hal-lxsis/src/asm.rs
use core::arch::asm;

/// Memory Wait — ensures all outstanding memory operations complete.
/// Equivalent to ARM DSB for load/store ordering.
#[inline(always)]
pub fn memw() {
    unsafe { asm!("memw", options(nostack, preserves_flags)) }
}

/// Instruction Sync — flushes the instruction pipeline.
/// Equivalent to ARM ISB.
#[inline(always)]
pub fn isync() {
    unsafe { asm!("isync", options(nostack, preserves_flags)) }
}

#[inline(always)]
pub fn nop()  { unsafe { asm!("nop",  options(nostack, preserves_flags)) } }

/// Wait For Interrupt — core enters low-power state until an interrupt fires.
#[inline(always)]
pub fn waiti(level: u32) {
    // WAITI sets PS.INTLEVEL to `level` and halts until an interrupt arrives.
    unsafe { asm!("waiti {0}", in(reg) level, options(nostack)) }
}
```

---

## 4. Interrupt level lock — `hal-lxsis/src/intlevel.rs`

Xtensa LX does not have a BASEPRI-style priority ceiling.  Instead the
`PS.INTLEVEL` field (bits [3:0] of the Processor State register) masks all
interrupts at that level and below.  `RSIL` atomically reads the old PS and
sets a new INTLEVEL; `WSR.PS` restores it.

```rust
// hal/hal-lxsis/src/intlevel.rs
use core::arch::asm;

/// Read PS and set PS.INTLEVEL to `level`.  Returns the previous PS value.
///
/// Interrupt levels on Xtensa (ESP32): 1–5 = normal, 6 = debug, 7 = NMI.
/// Setting level 5 masks all maskable interrupts.
///
/// # Safety — caller must restore with wsr_ps(prev).
#[inline(always)]
pub unsafe fn rsil(level: u32) -> u32 {
    let ps: u32;
    unsafe { asm!("rsil {0}, {1}", out(reg) ps, in(reg) level, options(nostack)) }
    ps
}

/// Write the Processor State register (restores a saved PS value).
#[inline(always)]
pub unsafe fn wsr_ps(ps: u32) {
    unsafe { asm!("wsr.ps {0}", in(reg) ps, options(nostack)) }
    // isync required after WSR.PS before the new level takes effect.
    unsafe { asm!("isync", options(nostack, preserves_flags)) }
}
```

### QK scheduler lock — `ports/xtensa/src/intlevel_cfg.rs`

```rust
// ports/xtensa/src/intlevel_cfg.rs
use hal_lxsis::intlevel::{rsil, wsr_ps};
use hal_lxsis::asm;

/// Interrupt level for the QK scheduler lock.
///
/// ISRs at level <= QK_INTLEVEL are masked while the scheduler runs.
/// ISRs at level > QK_INTLEVEL (higher urgency) are never masked.
///
/// Typical layout on ESP32 (5 maskable levels):
///   Level 1–2  QK-unaware ISRs (non-QF peripherals)
///   Level 3    QK_INTLEVEL ← ceiling
///   Level 4–5  QK-aware ISRs: tick timer, radio interrupt
///   Level 6    Debug / NMI — never masked
pub const QK_INTLEVEL: u32 = 3;

#[inline]
pub fn qk_lock() -> u32 {
    // SAFETY: restores with qk_unlock.
    let prev = unsafe { rsil(QK_INTLEVEL) };
    asm::memw();
    prev
}

#[inline]
pub fn qk_unlock(prev: u32) {
    asm::memw();
    unsafe { wsr_ps(prev) }
}
```

---

## 5. INTENABLE controller — `hal-lxsis/src/intenable.rs`

Xtensa LX interrupt enable/disable is done through the `INTENABLE` special
register (one bit per interrupt source, up to 32 sources).  Pending
interrupts are read from the `INTERRUPT` special register.

```rust
// hal/hal-lxsis/src/intenable.rs
use core::arch::asm;
use hal::error::{HalError, HalResult};
use hal::interrupt::{InterruptController, InterruptPriority};

fn read_intenable() -> u32 {
    let v: u32;
    unsafe { asm!("rsr.intenable {0}", out(reg) v, options(nomem, nostack)) }
    v
}

unsafe fn write_intenable(val: u32) {
    unsafe { asm!("wsr.intenable {0}", in(reg) val, options(nomem, nostack)) }
    unsafe { asm!("isync", options(nostack, preserves_flags)) }
}

fn read_interrupt() -> u32 {
    let v: u32;
    unsafe { asm!("rsr.interrupt {0}", out(reg) v, options(nomem, nostack)) }
    v
}

/// Set a software interrupt (if the source supports it).
unsafe fn write_intset(mask: u32) {
    unsafe { asm!("wsr.intset {0}", in(reg) mask, options(nomem, nostack)) }
}

/// Clear a software interrupt.
unsafe fn write_intclear(mask: u32) {
    unsafe { asm!("wsr.intclear {0}", in(reg) mask, options(nomem, nostack)) }
}

pub struct IntenableController { _private: () }

impl IntenableController {
    /// # Safety — must be constructed at most once per core.
    pub const unsafe fn new() -> Self { Self { _private: () } }
}

impl InterruptController for IntenableController {
    fn enable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 32 { return Err(HalError::InvalidParameter); }
        let cur = read_intenable();
        unsafe { write_intenable(cur | (1 << irq)) }
        Ok(())
    }

    fn disable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 32 { return Err(HalError::InvalidParameter); }
        let cur = read_intenable();
        unsafe { write_intenable(cur & !(1 << irq)) }
        Ok(())
    }

    fn set_priority(&mut self, _irq: u32, _priority: InterruptPriority) -> HalResult<()> {
        // Interrupt priority on ESP32 is fixed in hardware per interrupt source.
        // Reassignment requires reprogramming the interrupt matrix via peripheral
        // registers — handled in the BSP, not here.
        Ok(())
    }

    fn is_pending(&self, irq: u32) -> bool {
        if irq >= 32 { return false; }
        (read_interrupt() >> irq) & 1 != 0
    }

    fn clear_pending(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 32 { return Err(HalError::InvalidParameter); }
        unsafe { write_intclear(1 << irq) }
        Ok(())
    }
}
```

---

## 6. Tick timer — `hal-lxsis/src/ccompare.rs`

Xtensa LX has a free-running cycle counter (`CCOUNT`) and up to three
compare registers (`CCOMPARE0`, `CCOMPARE1`, `CCOMPARE2`).  When `CCOUNT`
equals `CCOMPAREn`, interrupt source `n+6` fires (on ESP32).

```rust
// hal/hal-lxsis/src/ccompare.rs
use core::arch::asm;
use hal::error::{HalError, HalResult};
use hal::timer::{Timer, TimerMode};

fn read_ccount() -> u32 {
    let v: u32;
    unsafe { asm!("rsr.ccount {0}", out(reg) v, options(nomem, nostack)) }
    v
}

fn read_ccompare0() -> u32 {
    let v: u32;
    unsafe { asm!("rsr.ccompare0 {0}", out(reg) v, options(nomem, nostack)) }
    v
}

unsafe fn write_ccompare0(val: u32) {
    unsafe { asm!("wsr.ccompare0 {0}", in(reg) val, options(nomem, nostack)) }
    unsafe { asm!("isync", options(nostack, preserves_flags)) }
}

pub struct CcompareTimer {
    core_mhz:  u32,
    period_cy: u32,   // period in cycles — used for periodic reload
    periodic:  bool,
}

impl CcompareTimer {
    pub const fn new(core_mhz: u32) -> Self {
        Self { core_mhz, period_cy: 0, periodic: false }
    }
}

impl Timer for CcompareTimer {
    fn start(&mut self, period_us: u64, mode: TimerMode) -> HalResult<()> {
        let cycles = (period_us * self.core_mhz as u64) as u32;
        if cycles == 0 { return Err(HalError::InvalidParameter); }
        self.period_cy = cycles;
        self.periodic  = matches!(mode, TimerMode::Periodic);
        let next = read_ccount().wrapping_add(cycles);
        unsafe { write_ccompare0(next) }
        Ok(())
    }

    fn stop(&mut self) -> HalResult<()> {
        // Writing CCOMPARE0 far ahead effectively disables the timer.
        unsafe { write_ccompare0(read_ccount().wrapping_add(u32::MAX / 2)) }
        Ok(())
    }

    fn counter(&self) -> u64 { read_ccount() as u64 }

    fn enable_interrupt(&mut self)  -> HalResult<()> { Ok(()) }  // enabled by INTENABLE bit 6
    fn disable_interrupt(&mut self) -> HalResult<()> { Ok(()) }  // disabled by INTENABLE bit 6

    fn clear_interrupt(&mut self) -> HalResult<()> {
        // Reload the compare register for the next period (periodic mode).
        if self.periodic {
            let next = read_ccompare0().wrapping_add(self.period_cy);
            unsafe { write_ccompare0(next) }
        }
        Ok(())
    }
}
```

The ISR body lives in `ports/xtensa/src/lib.rs` and calls
`timer.clear_interrupt()` (which reloads `CCOMPARE0`) then `runtime.tick()`.

---

## 7. Vendor peripheral pattern — `hal-lxsis/src/<chip>/`

Memory-mapped peripherals (GPIO, SPI, UART) follow the same pattern as
`hal-cmsis`: `regs.rs` with `RW<T>`/`RO<T>`/`WO<T>` structs, one file per
peripheral implementing the `hal` trait.

### ESP32 GPIO base addresses

```rust
// hal/hal-lxsis/src/esp32/regs.rs  (excerpt)
use hal::mmio::{RO, RW, WO};

pub const GPIO_BASE: usize = 0x3FF4_4000;

#[repr(C)]
pub struct GpioRegs {
    pub bt_select:   RW<u32>,   // 0x000  Bluetooth / reserved
    pub out:         RW<u32>,   // 0x004  GPIO output (GPIO 0–31)
    pub out_w1ts:    WO<u32>,   // 0x008  Output set (write-1-to-set)
    pub out_w1tc:    WO<u32>,   // 0x00C  Output clear (write-1-to-clear)
    pub out1:        RW<u32>,   // 0x010  GPIO 32–39 output
    pub out1_w1ts:   WO<u32>,   // 0x014
    pub out1_w1tc:   WO<u32>,   // 0x018
    _r0:             [u32; 2],
    pub enable:      RW<u32>,   // 0x020  Output enable (GPIO 0–31)
    pub enable_w1ts: WO<u32>,   // 0x024
    pub enable_w1tc: WO<u32>,   // 0x028
    pub enable1:     RW<u32>,   // 0x02C  GPIO 32–39 output enable
    // … further fields …
    pub in_:         RO<u32>,   // 0x03C  Input data (GPIO 0–31)
    pub in1:         RO<u32>,   // 0x040  Input data (GPIO 32–39)
}

pub fn gpio() -> &'static GpioRegs {
    unsafe { &*(GPIO_BASE as *const GpioRegs) }
}
```

ESP32 GPIO mode configuration goes through per-pin IO_MUX registers and the
GPIO matrix — different from STM32 MODER but the `GpioPin` trait interface
hides this completely.

---

## 8. `Cargo.toml` — feature flags

```toml
# hal/hal-lxsis/Cargo.toml
[dependencies]
hal = { path = ".." }
# No xtensa crate, no esp-idf bindings.  Zero external dependencies.

[features]
default  = []
esp32    = []     # Xtensa LX6 dual-core, 240 MHz
esp32s2  = []     # Xtensa LX7 single-core, 240 MHz
esp32s3  = []     # Xtensa LX7 dual-core, 240 MHz + AI accelerator
qp-integration = ["hal/qp-integration"]
```

---

## 9. Adding a new Xtensa LX chip

| Step | Action |
|---|---|
| 1 | Add feature to `hal-lxsis/Cargo.toml` |
| 2 | Create `hal-lxsis/src/<chip>/regs.rs` with peripheral register structs |
| 3 | Implement `GpioPin`, `SpiMaster`, `Uart` — same pattern as ESP32 |
| 4 | Check `CCOUNT` frequency and interrupt level mapping for the new chip |
| 5 | Set `QK_INTLEVEL` in `ports/xtensa/intlevel_cfg.rs` for the target's interrupt scheme |

---

## 10. Retargetability matrix

| Change | Files touched |
|---|---|
| New Xtensa LX chip | `hal-lxsis/Cargo.toml` + new `<chip>/` module |
| New peripheral type | `hal/src/<periph>.rs` (trait) + `<chip>/<periph>.rs` |
| Change tick rate | BSP: `CcompareTimer::start(period_us, …)` |
| Change interrupt level layout | `ports/xtensa/intlevel_cfg.rs` — `QK_INTLEVEL` |
| New radio PHY on ESP32 | `hal-lxsis/src/esp32/radio/<phy>.rs` implementing `hal::RfDriver` |
