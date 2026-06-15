# Pure Rust RISC-V Port (RVSIS)

This document covers `hal-rvsis` — the RISC-V member of the `*SIS` port
family.  For the cross-crate pattern shared by all three ports see
[CMSIS Port §2](./cmsis.md#2-the-sis-cross-crate-pattern).

Supported targets: **ESP32-C3** (RV32IMC), **ESP32-C6** (RV32IMAC + BT5/WiFi6),
**GD32VF103** (RV32IMAC), generic RISC-V MCUs with PLIC + CLINT.

---

## 1. RISC-V vs Cortex-M — key differences

| Aspect | Cortex-M (CMSIS) | RISC-V (RVSIS) |
|---|---|---|
| Interrupt controller | NVIC — memory-mapped at `0xE000_E100` | PLIC — memory-mapped at `0x0C00_0000` (standard); vendor-specific on ESP32-Cx |
| Tick timer | SysTick — memory-mapped at `0xE000_E010` | CLINT `mtime`/`mtimecmp` at `0x0200_0000` (standard) |
| Scheduler lock | `BASEPRI` / `PRIMASK` (ARM-specific registers) | `csrci MSTATUS, 8` — clear MIE bit in Machine Status CSR |
| Memory barrier | `dsb` / `dmb` | `fence iorw, iorw` |
| Instruction sync | `isb` | `fence.i` |
| Special register access | `MRS`/`MSR` instructions | `csrr`/`csrw`/`csrsi`/`csrci` instructions |
| Peripheral registers | Memory-mapped, `RW<T>` | Memory-mapped, same `hal/src/mmio.rs` `RW<T>` |

RISC-V splits sharply between the **M-mode privilege CSRs** (controlling
interrupts, timer, global state — accessed via `csr*` instructions) and the
**peripheral registers** (memory-mapped — use `hal/src/mmio.rs` exactly as in
`hal-cmsis`).

---

## 2. Repository layout

```
hal/
  src/
    mmio.rs          ← shared with hal-cmsis and hal-lxsis
    gpio.rs, spi.rs, uart.rs, timer.rs, interrupt.rs, error.rs

  hal-rvsis/
    Cargo.toml       feature per chip; dep on hal only
    src/
      lib.rs         re-exports; #![no_std]
      asm.rs         fence / fence_i / nop / wfi
      csr.rs         mstatus / mie / mip read/write helpers
      mstatus.rs     qk_lock / qk_unlock via MIE bit
      plic.rs        PlicRegs + PlicController : InterruptController
      clint.rs       ClintRegs + ClintTimer : Timer
      esp32c3/       #[cfg(feature = "esp32c3")]
        mod.rs
        regs.rs      GPIO / SPI / UART register structs
        gpio.rs      Esp32C3Pin : GpioPin
        spi.rs       Esp32C3Spi : SpiMaster
        uart.rs      Esp32C3Uart : Uart
        intmtx.rs    ESP32-C3 interrupt matrix (replaces standard PLIC)
      esp32c6/       #[cfg(feature = "esp32c6")]
        …            (PLIC-compatible; same plic.rs with different base address)
      gd32vf/        #[cfg(feature = "gd32vf103")]
        …

ports/
  riscv/             ← QXK scheduler glue for RISC-V
    src/
      lib.rs         Machine-mode trap handler; software interrupt for context switch
      context.rs     ContextFrame — caller-saved register layout per RISC-V ABI
      mstatus_cfg.rs QK lock/unlock via MIE
```

---

## 3. CSR access — `hal-rvsis/src/csr.rs`

RISC-V Control and Status Registers are accessed with dedicated instructions,
not memory loads/stores.  The register name is encoded as an immediate
operand (12-bit CSR address).

```rust
// hal/hal-rvsis/src/csr.rs
use core::arch::asm;

// Selected CSR addresses (RISC-V privileged spec).
pub const MSTATUS: u32 = 0x300;
pub const MIE:     u32 = 0x304;
pub const MIP:     u32 = 0x344;
pub const MCAUSE:  u32 = 0x342;
pub const MEPC:    u32 = 0x341;
pub const MTVEC:   u32 = 0x305;

/// Read a CSR.
#[inline(always)]
pub fn csrr(csr: u32) -> u32 {
    let val: u32;
    // The CSR address must be a compile-time constant for the `csrr` instruction.
    // Use the macro below for a const-friendly version.
    unsafe { asm!("csrr {0}, {1}", out(reg) val, const csr, options(nomem, nostack)) }
    val
}

/// Write a CSR.
/// # Safety — writing CSRs changes machine-mode privilege state.
#[inline(always)]
pub unsafe fn csrw(csr: u32, val: u32) {
    unsafe { asm!("csrw {0}, {1}", const csr, in(reg) val, options(nomem, nostack)) }
}

/// Atomic read-and-set bits in a CSR.
#[inline(always)]
pub unsafe fn csrrs(csr: u32, mask: u32) -> u32 {
    let old: u32;
    unsafe { asm!("csrrs {0}, {1}, {2}", out(reg) old, const csr, in(reg) mask, options(nomem, nostack)) }
    old
}

/// Atomic read-and-clear bits in a CSR.
#[inline(always)]
pub unsafe fn csrrc(csr: u32, mask: u32) -> u32 {
    let old: u32;
    unsafe { asm!("csrrc {0}, {1}, {2}", out(reg) old, const csr, in(reg) mask, options(nomem, nostack)) }
    old
}
```

---

## 4. Barrier / wait instructions — `hal-rvsis/src/asm.rs`

```rust
// hal/hal-rvsis/src/asm.rs
use core::arch::asm;

/// Full memory fence — orders all memory operations on all devices.
/// Equivalent to ARM DSB + DMB combined.
#[inline(always)]
pub fn fence() {
    unsafe { asm!("fence iorw, iorw", options(nostack, preserves_flags)) }
}

/// Instruction fence — synchronises instruction stream with data memory.
/// Equivalent to ARM ISB.
#[inline(always)]
pub fn fence_i() {
    unsafe { asm!("fence.i", options(nostack, preserves_flags)) }
}

#[inline(always)]
pub fn nop() { unsafe { asm!("nop", options(nostack, preserves_flags)) } }

/// Wait For Interrupt — stall until an interrupt or event wakes the core.
#[inline(always)]
pub fn wfi() { unsafe { asm!("wfi", options(nostack, preserves_flags)) } }
```

---

## 5. QK scheduler lock — `ports/riscv/src/mstatus_cfg.rs`

RISC-V does not have a priority-ceiling mechanism equivalent to `BASEPRI`.
The scheduler lock clears the global Machine Interrupt Enable bit (`MIE` in
`MSTATUS`), masking all M-mode interrupts.

```rust
// ports/riscv/src/mstatus_cfg.rs
use hal_rvsis::csr::{csrrc, csrrs, MSTATUS};
use hal_rvsis::asm;

const MSTATUS_MIE: u32 = 1 << 3;   // Machine Interrupt Enable

/// Lock the QK scheduler — disable all M-mode interrupts.
/// Returns the previous MSTATUS value for restore.
#[inline]
pub fn qk_lock() -> u32 {
    // csrrc atomically reads MSTATUS and clears MIE.
    let prev = unsafe { csrrc(MSTATUS, MSTATUS_MIE) };
    asm::fence();
    prev
}

/// Unlock the QK scheduler — restore MSTATUS to its previous value.
#[inline]
pub fn qk_unlock(prev: u32) {
    asm::fence();
    if prev & MSTATUS_MIE != 0 {
        unsafe { csrrs(MSTATUS, MSTATUS_MIE) };
    }
}
```

For platforms that implement the RISC-V interrupt priority extension (`Smaia`
/ `Smepmp`) a partial-mask approach similar to BASEPRI becomes possible, but
it is not yet available on ESP32-Cx targets and is out of scope here.

---

## 6. PLIC — `hal-rvsis/src/plic.rs`

The Platform-Level Interrupt Controller is the standard RISC-V external
interrupt controller.  It handles interrupt priority, enable, pending, and
claim/complete.

```rust
// hal/hal-rvsis/src/plic.rs
use hal::mmio::{RO, RW, WO};
use hal::error::{HalError, HalResult};
use hal::interrupt::{InterruptController, InterruptPriority};

// Standard RISC-V PLIC layout (SiFive / RISC-V spec).
// Vendors may use a different base address — set via feature flag or const generic.
pub const PLIC_BASE_DEFAULT: usize = 0x0C00_0000;

#[repr(C)]
pub struct PlicRegs {
    pub priority:  [RW<u32>; 1024],  // 0x000000  Source priority (0 = disabled)
    pub pending:   [RO<u32>; 32],    // 0x001000  Interrupt pending bits
    _r0:           [u32; 992],
    pub enable:    [[RW<u32>; 32]; 15872], // 0x002000  Enable bits per context
    _r1:           [u32; 0x1F_C000 / 4],
    // Per-context threshold + claim/complete at 0x200000 + 0x1000 * context
}

/// Lightweight PLIC handle — only the fields qp-rs actually needs.
pub struct PlicController {
    base:    usize,
    context: usize,   // Hart context index (0 = hart 0 M-mode)
}

impl PlicController {
    /// # Safety — `base` must be the PLIC base address for this target;
    /// `context` must be the RISC-V hart context index (0 for single-hart M-mode).
    pub const unsafe fn new(base: usize, context: usize) -> Self {
        Self { base, context }
    }

    fn priority_reg(&self, irq: u32) -> *mut u32 {
        (self.base + 4 * irq as usize) as *mut u32
    }

    fn enable_reg(&self, irq: u32) -> *mut u32 {
        (self.base + 0x2000 + self.context * 0x80 + 4 * (irq / 32) as usize) as *mut u32
    }

    fn threshold_reg(&self) -> *mut u32 {
        (self.base + 0x20_0000 + self.context * 0x1000) as *mut u32
    }

    fn claim_reg(&self) -> *mut u32 {
        (self.base + 0x20_0004 + self.context * 0x1000) as *mut u32
    }
}

impl InterruptController for PlicController {
    fn enable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq == 0 || irq >= 1024 { return Err(HalError::InvalidParameter); }
        let reg = self.enable_reg(irq);
        let cur = unsafe { core::ptr::read_volatile(reg) };
        unsafe { core::ptr::write_volatile(reg, cur | (1 << (irq % 32))) }
        Ok(())
    }

    fn disable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq == 0 || irq >= 1024 { return Err(HalError::InvalidParameter); }
        let reg = self.enable_reg(irq);
        let cur = unsafe { core::ptr::read_volatile(reg) };
        unsafe { core::ptr::write_volatile(reg, cur & !(1 << (irq % 32))) }
        Ok(())
    }

    fn set_priority(&mut self, irq: u32, priority: InterruptPriority) -> HalResult<()> {
        if irq == 0 || irq >= 1024 { return Err(HalError::InvalidParameter); }
        unsafe { core::ptr::write_volatile(self.priority_reg(irq), priority as u32) }
        Ok(())
    }

    fn is_pending(&self, irq: u32) -> bool {
        if irq == 0 || irq >= 1024 { return false; }
        let reg = (self.base + 0x1000 + 4 * (irq / 32) as usize) as *const u32;
        (unsafe { core::ptr::read_volatile(reg) } >> (irq % 32)) & 1 != 0
    }

    fn clear_pending(&mut self, irq: u32) -> HalResult<()> {
        // PLIC clears pending via claim/complete cycle — not a direct write.
        // The ISR wrapper must call claim() on entry and complete() on exit.
        let _ = irq;
        Ok(())
    }
}

impl PlicController {
    /// Claim the highest-priority pending interrupt.  Returns the IRQ number.
    /// Call at ISR entry; the PLIC will not re-present this IRQ until complete().
    pub fn claim(&self) -> u32 {
        unsafe { core::ptr::read_volatile(self.claim_reg()) }
    }

    /// Signal completion of the IRQ returned by claim().
    pub fn complete(&self, irq: u32) {
        unsafe { core::ptr::write_volatile(self.claim_reg(), irq) }
    }
}
```

---

## 7. CLINT timer — `hal-rvsis/src/clint.rs`

The Core-Local Interruptor provides `mtime` (64-bit free-running counter) and
`mtimecmp` (64-bit compare register).  A timer interrupt fires whenever
`mtime >= mtimecmp`.

```rust
// hal/hal-rvsis/src/clint.rs
use hal::error::{HalError, HalResult};
use hal::timer::{Timer, TimerMode};

pub const CLINT_BASE_DEFAULT: usize = 0x0200_0000;

const MTIME_OFFSET:    usize = 0xBFF8;
const MTIMECMP_OFFSET: usize = 0x4000;

pub struct ClintTimer {
    base:      usize,
    hz:        u64,    // mtime frequency (Hz)
    period_cy: u64,
    periodic:  bool,
}

impl ClintTimer {
    /// `base` — CLINT base address.  `hz` — mtime counter frequency (often 1 MHz
    /// on SoCs; read from the device tree or a fixed constant per target).
    pub const fn new(base: usize, hz: u64) -> Self {
        Self { base, hz, period_cy: 0, periodic: false }
    }

    fn mtime(&self) -> u64 {
        let lo = unsafe { core::ptr::read_volatile((self.base + MTIME_OFFSET)     as *const u32) };
        let hi = unsafe { core::ptr::read_volatile((self.base + MTIME_OFFSET + 4) as *const u32) };
        (hi as u64) << 32 | lo as u64
    }

    fn set_mtimecmp(&self, val: u64) {
        // Write hi first with MAX to avoid spurious interrupt, then lo, then hi.
        unsafe { core::ptr::write_volatile((self.base + MTIMECMP_OFFSET + 4) as *mut u32, u32::MAX) }
        unsafe { core::ptr::write_volatile((self.base + MTIMECMP_OFFSET)     as *mut u32, val as u32) }
        unsafe { core::ptr::write_volatile((self.base + MTIMECMP_OFFSET + 4) as *mut u32, (val >> 32) as u32) }
    }
}

impl Timer for ClintTimer {
    fn start(&mut self, period_us: u64, mode: TimerMode) -> HalResult<()> {
        let cycles = period_us * self.hz / 1_000_000;
        if cycles == 0 { return Err(HalError::InvalidParameter); }
        self.period_cy = cycles;
        self.periodic  = matches!(mode, TimerMode::Periodic);
        self.set_mtimecmp(self.mtime() + cycles);
        Ok(())
    }

    fn stop(&mut self) -> HalResult<()> {
        self.set_mtimecmp(u64::MAX);
        Ok(())
    }

    fn counter(&self) -> u64 { self.mtime() }

    fn enable_interrupt(&mut self)  -> HalResult<()> { Ok(()) }  // controlled via MIE.MTIE CSR
    fn disable_interrupt(&mut self) -> HalResult<()> { Ok(()) }

    fn clear_interrupt(&mut self) -> HalResult<()> {
        if self.periodic {
            self.set_mtimecmp(self.mtime() + self.period_cy);
        }
        Ok(())
    }
}
```

---

## 8. ESP32-Cx interrupt matrix note

ESP32-C3 and ESP32-C6 do not expose a standard PLIC.  They use Espressif's
**interrupt matrix** — a memory-mapped crossbar that routes any of the ~62
peripheral interrupt sources to any of the CPU's 31 interrupt inputs, each
of which has a configurable level (1–7) and type (level/edge).

The `InterruptController` trait maps cleanly onto this:

- `enable_interrupt(irq)` → write to the interrupt matrix routing register for
  that peripheral source, assigning it to a CPU interrupt input.
- `set_priority(irq, priority)` → write to the CPU interrupt priority register.
- `is_pending` / `clear_pending` → read/write `INTR_STATUS` registers.

`hal-rvsis/src/esp32c3/intmtx.rs` implements `InterruptController` for this
hardware.  The `plic.rs` module is not used on these targets — the feature
flag selects the right implementation automatically.

---

## 9. `Cargo.toml` — feature flags

```toml
# hal/hal-rvsis/Cargo.toml
[dependencies]
hal = { path = ".." }
# No riscv crate, no esp-idf bindings.  Zero external dependencies.

[features]
default  = []
esp32c3  = []     # RV32IMC, 160 MHz, custom interrupt matrix
esp32c6  = []     # RV32IMAC + BLE5 + IEEE802.15.4, custom interrupt matrix
gd32vf103 = []    # RV32IMAC, standard PLIC + CLINT
qp-integration = ["hal/qp-integration"]
```

---

## 10. Adding a new RISC-V chip

| Step | Action |
|---|---|
| 1 | Add feature to `hal-rvsis/Cargo.toml` |
| 2 | Create `hal-rvsis/src/<chip>/regs.rs` with peripheral register structs |
| 3 | If chip uses standard PLIC: instantiate `PlicController` with the chip's base address |
| 4 | If chip uses a custom interrupt matrix: implement `InterruptController` in `<chip>/intmtx.rs` |
| 5 | If chip uses standard CLINT: instantiate `ClintTimer` with base address + mtime Hz |
| 6 | Implement `GpioPin`, `SpiMaster`, `Uart` in `<chip>/gpio.rs` etc. |
| 7 | In BSP: call timer `start()` and set up interrupt routing |

---

## 11. Retargetability matrix

| Change | Files touched |
|---|---|
| New RISC-V chip (standard PLIC + CLINT) | `hal-rvsis/Cargo.toml` + `<chip>/` (peripheral regs only) |
| New RISC-V chip (custom interrupt matrix) | `hal-rvsis/Cargo.toml` + `<chip>/` + `<chip>/intmtx.rs` |
| New peripheral type | `hal/src/<periph>.rs` (trait) + `<chip>/<periph>.rs` |
| Change tick rate | BSP: `ClintTimer::start(period_us, …)` |
| Change scheduler lock granularity | `ports/riscv/mstatus_cfg.rs` — upgrade to `Smaia` when available |
| New radio PHY on RISC-V | `hal-rvsis/src/<chip>/radio/<phy>.rs` implementing `hal::RfDriver` |
| Port to Xtensa LX | See [LXSIS](./lxsis.md) |
| Port to Cortex-M | See [CMSIS](./cmsis.md) |
