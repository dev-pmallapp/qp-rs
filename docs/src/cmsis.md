# Pure Rust CMSIS Port (Cortex-M)

This document covers `hal-cmsis` — the Cortex-M member of the `*SIS` port
family.  The other members are [`hal-lxsis`](./lxsis.md) (Xtensa LX) and
[`hal-rvsis`](./rvsis.md) (RISC-V).  All three share the same `mmio`
register-access primitives from `hal/src/mmio.rs` and implement the same
`hal/src/` traits.

---

## 1. What this port covers

CMSIS (Cortex Microcontroller Software Interface Standard) defines the
processor-level API for all ARM Cortex-M devices.  This port reimplements
that API in pure Rust — no FFI, no C headers, no `cortex-m` crate dependency.

| CMSIS-Core component | Pure Rust location |
|---|---|
| NVIC (interrupt enable/priority/pending) | `hal-cmsis/src/nvic.rs` |
| SysTick (tick source) | `hal-cmsis/src/systick.rs` |
| SCB (cache, VTOR) | `hal-cmsis/src/scb.rs` |
| BASEPRI (scheduler lock, M3/M4/M7) | `hal-cmsis/src/basepri.rs` |
| Barriers: DSB / ISB / DMB | `hal-cmsis/src/asm.rs` |
| Wait: WFI / WFE | `hal-cmsis/src/asm.rs` |
| Vendor peripheral registers | `hal-cmsis/src/<chip>/regs.rs` |

CMSIS-RTOS2, CMSIS-DSP, and CMSIS-NN are out of scope — qp-rs is the RTOS.

---

## 2. The `*SIS` cross-crate pattern

All three ports (`hal-cmsis`, `hal-lxsis`, `hal-rvsis`) follow the same
structural rules so that adding a new vendor or peripheral type is
mechanical:

1. **Shared register access** via `hal/src/mmio.rs` (`RW<T>`, `RO<T>`,
   `WO<T>`).  No port defines its own volatile wrappers.

2. **Shared trait targets** from `hal/src/` (`GpioPin`, `SpiMaster`, `Uart`,
   `Timer`, `InterruptController`).  The port's job is to implement these,
   nothing more.

3. **Chip families selected by Cargo feature flag** — one feature per chip,
   one module per chip, zero runtime dispatch.

4. **Per-chip module layout** is always:
   ```
   <port>/src/<chip>/
     mod.rs      pub use gpio::…;  pub use spi::…;
     regs.rs     register block structs using RW<T>/RO<T>/WO<T>
     gpio.rs     impl GpioPin for …
     spi.rs      impl SpiMaster for …
     uart.rs     impl Uart for …
   ```

5. **Processor-core modules** (interrupt controller, tick timer, barriers,
   scheduler lock) are architecture-specific and live directly under
   `<port>/src/`, not under `<chip>/`.

---

## 3. Repository layout

```
hal/
  src/                           ← trait definitions + shared primitives
    mmio.rs      RW<T>/RO<T>/WO<T>  ← used by all *SIS ports
    gpio.rs      GpioPin / GpioPinInterrupt
    spi.rs       SpiMaster / SpiDevice
    uart.rs      Uart
    timer.rs     Timer / TimerMode
    interrupt.rs InterruptController / InterruptPriority
    error.rs     HalError / HalResult

  hal-cmsis/                     ← this port (Cortex-M)
    Cargo.toml   feature per chip family; dep on hal only
    src/
      lib.rs     re-exports; #![no_std]
      asm.rs     dsb / isb / dmb / nop / wfi / wfe
      basepri.rs read_basepri / write_basepri (ARMv7-M only)
      nvic.rs    NvicRegs + NvicController : InterruptController
      scb.rs     ScbRegs + cache ops
      systick.rs SysTickRegs + SysTickTimer : Timer
      stm32f4/   #[cfg(feature = "stm32f4xx")]
      nrf52/     #[cfg(feature = "nrf52840")]
      lpc17/     #[cfg(feature = "lpc1768")]

  hal-lxsis/                     ← Xtensa LX port (see lxsis.md)
  hal-rvsis/                     ← RISC-V port (see rvsis.md)

ports/
  cortex-m/
    src/
      lib.rs       PendSV / SVC handlers; SysTick ISR
      context.rs   ContextFrame — exception stack frame layout
      nvic_cfg.rs  QK_BASEPRI + qk_lock / qk_unlock
```

**Layering invariant** — no arrow points upward:

```
Application
    ↓
qf / qk / qxk / comms / qs
    ↓
ports/cortex-m
    ↓
hal/src/          (traits + mmio)
    ↓
hal-cmsis/        (register access — depends only on hal/src/ and core::)
```

---

## 4. Processor-core implementation

### NVIC — `hal-cmsis/src/nvic.rs`

```rust
use hal::mmio::{RO, RW};   // from hal/src/mmio.rs
use hal::error::{HalError, HalResult};
use hal::interrupt::{InterruptController, InterruptPriority};

const NVIC_BASE: usize = 0xE000_E100;

#[repr(C)]
pub struct NvicRegs {
    pub iser: [RW<u32>; 8],   // 0x000  Interrupt Set-Enable
    _r0:      [u32; 24],
    pub icer: [RW<u32>; 8],   // 0x080  Interrupt Clear-Enable
    _r1:      [u32; 24],
    pub ispr: [RW<u32>; 8],   // 0x100  Interrupt Set-Pending
    _r2:      [u32; 24],
    pub icpr: [RW<u32>; 8],   // 0x180  Interrupt Clear-Pending
    _r3:      [u32; 24],
    pub iabr: [RO<u32>; 8],   // 0x200  Active Bit (read-only)
    _r4:      [u32; 56],
    pub ipr:  [RW<u8>; 240],  // 0x300  Priority (byte per IRQ)
}

fn nvic() -> &'static NvicRegs {
    unsafe { &*(NVIC_BASE as *const NvicRegs) }
}

pub struct NvicController { _private: () }

impl NvicController {
    /// # Safety — must be constructed at most once per core.
    pub const unsafe fn new() -> Self { Self { _private: () } }
}

impl InterruptController for NvicController {
    fn enable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 240 { return Err(HalError::InvalidParameter); }
        nvic().iser[(irq / 32) as usize].write(1 << (irq % 32));
        Ok(())
    }

    fn disable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 240 { return Err(HalError::InvalidParameter); }
        nvic().icer[(irq / 32) as usize].write(1 << (irq % 32));
        crate::asm::dsb();
        crate::asm::isb();
        Ok(())
    }

    fn set_priority(&mut self, irq: u32, priority: InterruptPriority) -> HalResult<()> {
        if irq >= 240 { return Err(HalError::InvalidParameter); }
        nvic().ipr[irq as usize].write(priority);
        Ok(())
    }

    fn is_pending(&self, irq: u32) -> bool {
        if irq >= 240 { return false; }
        (nvic().ispr[(irq / 32) as usize].read() >> (irq % 32)) & 1 != 0
    }

    fn clear_pending(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 240 { return Err(HalError::InvalidParameter); }
        nvic().icpr[(irq / 32) as usize].write(1 << (irq % 32));
        Ok(())
    }
}
```

### SysTick — `hal-cmsis/src/systick.rs`

```rust
use hal::mmio::{RO, RW};
use hal::timer::{Timer, TimerMode};
use hal::error::{HalError, HalResult};

const SYSTICK_BASE: usize = 0xE000_E010;

#[repr(C)]
pub struct SysTickRegs {
    pub csr:   RW<u32>,  // Control and Status
    pub rvr:   RW<u32>,  // Reload Value (24-bit)
    pub cvr:   RW<u32>,  // Current Value — write clears to 0
    pub calib: RO<u32>,  // Calibration
}

const CSR_ENABLE:    u32 = 1 << 0;
const CSR_TICKINT:   u32 = 1 << 1;
const CSR_CLKSOURCE: u32 = 1 << 2;  // 1 = processor clock

fn systick() -> &'static SysTickRegs {
    unsafe { &*(SYSTICK_BASE as *const SysTickRegs) }
}

pub struct SysTickTimer { core_mhz: u32 }

impl SysTickTimer {
    pub const fn new(core_mhz: u32) -> Self { Self { core_mhz } }
}

impl Timer for SysTickTimer {
    fn start(&mut self, period_us: u64, _mode: TimerMode) -> HalResult<()> {
        let ticks = ((period_us * self.core_mhz as u64) as u32).saturating_sub(1);
        if ticks == 0 || ticks > 0x00FF_FFFF { return Err(HalError::InvalidParameter); }
        let st = systick();
        st.csr.write(0);
        st.rvr.write(ticks);
        st.cvr.write(0);
        st.csr.write(CSR_ENABLE | CSR_TICKINT | CSR_CLKSOURCE);
        Ok(())
    }

    fn stop(&mut self) -> HalResult<()> { systick().csr.write(0); Ok(()) }
    fn counter(&self)  -> u64           { systick().cvr.read() as u64 }

    fn enable_interrupt(&mut self)  -> HalResult<()> { systick().csr.modify(|v| v | CSR_TICKINT);  Ok(()) }
    fn disable_interrupt(&mut self) -> HalResult<()> { systick().csr.modify(|v| v & !CSR_TICKINT); Ok(()) }
    fn clear_interrupt(&mut self)   -> HalResult<()> { let _ = systick().csr.read(); Ok(()) }
}
```

### BASEPRI — `hal-cmsis/src/basepri.rs`

Present only on ARMv7-M (M3/M4/M7/M33).  M0/M0+ use `PRIMASK` instead —
see §5 below.

```rust
use core::arch::asm;

#[inline(always)]
pub fn read() -> u8 {
    let v: u32;
    unsafe { asm!("mrs {}, BASEPRI", out(reg) v, options(nomem, nostack, preserves_flags)) }
    v as u8
}

/// # Safety — caller must restore with a matching write.
#[inline(always)]
pub unsafe fn write(val: u8) {
    unsafe { asm!("msr BASEPRI, {}", in(reg) val as u32, options(nomem, nostack, preserves_flags)) }
}
```

### Barrier / wait instructions — `hal-cmsis/src/asm.rs`

```rust
use core::arch::asm;

#[inline(always)] pub fn dsb() { unsafe { asm!("dsb", options(nostack, preserves_flags)) } }
#[inline(always)] pub fn isb() { unsafe { asm!("isb", options(nostack, preserves_flags)) } }
#[inline(always)] pub fn dmb() { unsafe { asm!("dmb", options(nostack, preserves_flags)) } }
#[inline(always)] pub fn nop() { unsafe { asm!("nop", options(nostack, preserves_flags)) } }
#[inline(always)] pub fn wfi() { unsafe { asm!("wfi", options(nostack, preserves_flags)) } }
#[inline(always)] pub fn wfe() { unsafe { asm!("wfe", options(nostack, preserves_flags)) } }
```

---

## 5. QK scheduler lock — `ports/cortex-m/src/nvic_cfg.rs`

```rust
use hal_cmsis::basepri;
use hal_cmsis::asm;

/// Priority ceiling for the QK scheduler lock.
///
/// ISRs with numerical priority < QK_BASEPRI are never masked.
/// ISRs at >= QK_BASEPRI are masked during the scheduler critical section
/// and are the only ISRs permitted to call post_from_isr().
pub const QK_BASEPRI: u8 = 0x50;

// ARMv7-M: use BASEPRI for priority ceiling (preferred — only masks low-urgency ISRs).
#[cfg(armv7m)]
pub fn qk_lock() -> u8 {
    let prev = basepri::read();
    unsafe { basepri::write(QK_BASEPRI) }
    asm::dsb(); asm::isb();
    prev
}

#[cfg(armv7m)]
pub fn qk_unlock(prev: u8) {
    unsafe { basepri::write(prev) }
    asm::dsb(); asm::isb();
}

// ARMv6-M (M0/M0+): no BASEPRI — fall back to PRIMASK (masks all interrupts).
#[cfg(armv6m)]
pub fn qk_lock() -> u8 {
    let prev: u32;
    unsafe { core::arch::asm!("mrs {}, PRIMASK", out(reg) prev) }
    unsafe { core::arch::asm!("cpsid i") }
    prev as u8
}

#[cfg(armv6m)]
pub fn qk_unlock(prev: u8) {
    unsafe { core::arch::asm!("msr PRIMASK, {}", in(reg) prev as u32) }
}
```

---

## 6. Vendor peripheral pattern — `hal-cmsis/src/<chip>/`

`regs.rs` defines register block structs using `hal::mmio::{RW, RO, WO}`.
One file per peripheral implements the matching `hal` trait.

### STM32F4 GPIO (`stm32f4/gpio.rs`)

```rust
use hal::mmio::{RO, RW, WO};
use hal::gpio::{GpioPin, Level, PinMode};
use hal::error::HalResult;

#[repr(C)]
pub struct GpioRegs {
    pub moder:   RW<u32>,  // Mode
    pub otyper:  RW<u32>,  // Output type
    pub ospeedr: RW<u32>,  // Speed
    pub pupdr:   RW<u32>,  // Pull-up/down
    pub idr:     RO<u32>,  // Input data
    pub odr:     RW<u32>,  // Output data
    pub bsrr:    WO<u32>,  // Bit set/reset (atomic)
    pub lckr:    RW<u32>,  // Lock
    pub afrl:    RW<u32>,  // Alternate function low
    pub afrh:    RW<u32>,  // Alternate function high
}

pub struct Stm32F4Pin { regs: *const GpioRegs, pin: u8 }
unsafe impl Send for Stm32F4Pin {}

impl Stm32F4Pin {
    /// # Safety — unique ownership of this port+pin must be guaranteed by the caller.
    pub unsafe fn new(regs: *const GpioRegs, pin: u8) -> Self { Self { regs, pin } }
    fn regs(&self) -> &GpioRegs { unsafe { &*self.regs } }
}

impl GpioPin for Stm32F4Pin {
    fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
        let shift = (self.pin as u32) * 2;
        let moder = match mode {
            PinMode::Input | PinMode::InputPullUp | PinMode::InputPullDown => 0b00,
            PinMode::Output | PinMode::OutputOpenDrain                      => 0b01,
            PinMode::Alternate(_)                                           => 0b10,
        };
        self.regs().moder.modify(|v| (v & !(0b11 << shift)) | (moder << shift));
        let pupdr = match mode {
            PinMode::InputPullUp   => 0b01u32,
            PinMode::InputPullDown => 0b10,
            _                      => 0b00,
        };
        self.regs().pupdr.modify(|v| (v & !(0b11 << shift)) | (pupdr << shift));
        let otype = if mode == PinMode::OutputOpenDrain { 1u32 } else { 0 };
        self.regs().otyper.modify(|v| (v & !(1 << self.pin)) | (otype << self.pin));
        Ok(())
    }

    fn read(&self) -> HalResult<Level> {
        Ok(if (self.regs().idr.read() >> self.pin) & 1 != 0 { Level::High } else { Level::Low })
    }

    fn write(&mut self, level: Level) -> HalResult<()> {
        let mask = match level {
            Level::High => 1u32 << self.pin,
            Level::Low  => 1u32 << (self.pin + 16),
        };
        self.regs().bsrr.write(mask);
        Ok(())
    }

    fn pin_number(&self) -> u32 { self.pin as u32 }
}
```

---

## 7. `Cargo.toml` — feature flags

```toml
# hal/hal-cmsis/Cargo.toml
[dependencies]
hal = { path = ".." }
# No cortex-m crate.  No svd2rust PACs.  Zero external dependencies.

[features]
default  = []
stm32f4xx = []
nrf52840  = []
lpc1768   = []
qp-integration = ["hal/qp-integration"]
```

---

## 8. SysTick ISR — tick source contract

The timer hardware is configured by `SysTickTimer::start()` in `hal-cmsis`.
The ISR body lives in `ports/cortex-m` so it can call `runtime.tick()`:

```rust
// ports/cortex-m/src/lib.rs
#[cfg(feature = "hw")]
#[no_mangle]
pub unsafe extern "C" fn SysTick_Handler() {
    qk_isr_entry!();
    if let Some(k) = KERNEL_PTR.as_ref() { let _ = k.lock().tick(); }
    qk_isr_exit!();
}
```

`hal-cmsis` never imports from `qf` or `qk`.

---

## 9. DMA / D-cache coherency (Cortex-M7)

```rust
// hal-cmsis/src/scb.rs  (future — needed for RF_STACK_PLAN DMA path)

/// Flush cache lines covering `buf` to SRAM before a DMA TX.
/// # Safety — buf must be 32-byte aligned (M7 cache line size).
pub unsafe fn clean_dcache(buf: &[u8]) {
    let (mut addr, end) = (buf.as_ptr() as usize & !0x1F, buf.as_ptr() as usize + buf.len());
    while addr < end {
        core::ptr::write_volatile((0xE000_ED68) as *mut u32, addr as u32); // DCCMVAC
        addr += 32;
    }
    crate::asm::dsb(); crate::asm::isb();
}

/// Invalidate cache lines covering `buf` after a DMA RX.
pub unsafe fn invalidate_dcache(buf: &[u8]) {
    let (mut addr, end) = (buf.as_ptr() as usize & !0x1F, buf.as_ptr() as usize + buf.len());
    while addr < end {
        core::ptr::write_volatile((0xE000_ED6C) as *mut u32, addr as u32); // DCIMVAC
        addr += 32;
    }
    crate::asm::dsb(); crate::asm::isb();
}
```

---

## 10. Adding a new Cortex-M chip

| Step | Action |
|---|---|
| 1 | Add feature + dependency to `hal-cmsis/Cargo.toml` |
| 2 | Create `hal-cmsis/src/<chip>/regs.rs` with register structs |
| 3 | Implement `GpioPin`, `SpiMaster`, `Uart` in `<chip>/gpio.rs` etc. |
| 4 | Gate with `#[cfg(feature = "<chip>")]`; re-export from `lib.rs` |
| 5 | In BSP: call `SysTickTimer::new(core_mhz)`, `NvicController::new()` |
| 6 | Set ISR priorities via `nvic.set_priority(…)` |

Nothing in `qf`, `qk`, `qxk`, `comms`, or any port changes.

---

## 11. Retargetability matrix

| Change | Files touched |
|---|---|
| New Cortex-M chip family | `hal-cmsis/Cargo.toml` + new `<chip>/` module |
| New peripheral type (e.g. I²C) | `hal/src/i2c.rs` (trait) + `<chip>/i2c.rs` per chip |
| New radio PHY on Cortex-M | `hal-cmsis/src/<chip>/radio/<phy>.rs` |
| Change tick rate | BSP: `SysTickTimer::start(period_us, …)` |
| Change ISR priority layout | BSP + `QK_BASEPRI` in `ports/cortex-m/nvic_cfg.rs` |
| Port to non-ARM architecture | See [LXSIS](./lxsis.md) / [RVSIS](./rvsis.md) |
