//! ARMv7-M Memory Protection Unit (MPU) configuration — spatial isolation
//! (see `docs/FUSA.md`, Phase 5).
//!
//! The MPU enforces, in hardware, the spatial-separation assumptions the
//! functional-safety case relies on. This port uses it for two things:
//!
//! - **Per-task stack guard regions** — a small *no-access* region placed at the
//!   limit (low address) of each active-object / extended-thread stack. A stack
//!   overflow then faults synchronously (MemManage) instead of silently
//!   corrupting adjacent memory — converting a latent spatial fault into the
//!   crash-only path ([`qf::fusa::on_error`]) via the MemManage handler.
//! - **Read-only regions for state tables** — `.rodata` and `const` HFSM state
//!   tables are marked read-only + execute-never, so a wild write cannot mutate
//!   the state machine and data cannot be executed as code (W^X).
//!
//! ## Design
//!
//! Region *descriptor computation* (the `RBAR`/`RASR` bit-packing) is pure and
//! lives in [`RegionConfig`]; it is unit-tested on the host. The *register
//! writes* that actually program the MPU live behind the `hw` feature in
//! [`Mpu`], so the host build compiles and tests the encoding without touching
//! hardware — mirroring the rest of this port.
//!
//! Traceability: ASR-008 (spatial memory isolation); see `docs/traceability.md`.

// ARMv7-M MPU register block (PPB, 0xE000_ED90..).
#[cfg(feature = "hw")]
const MPU_CTRL: *mut u32 = 0xE000_ED94 as *mut u32;
#[cfg(feature = "hw")]
const MPU_RNR: *mut u32 = 0xE000_ED98 as *mut u32;
#[cfg(feature = "hw")]
const MPU_RBAR: *mut u32 = 0xE000_ED9C as *mut u32;
#[cfg(feature = "hw")]
const MPU_RASR: *mut u32 = 0xE000_EDA0 as *mut u32;
/// SCB System Handler Control & State — `MEMFAULTENA` bit enables MemManage.
#[cfg(feature = "hw")]
const SCB_SHCSR: *mut u32 = 0xE000_ED24 as *mut u32;

/// Access permission for an MPU region (the `AP` field of `RASR`, bits 26:24).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    /// No access at any privilege level — used for stack-guard regions.
    NoAccess,
    /// Read-only at any privilege level.
    ReadOnly,
    /// Read/write at any privilege level.
    ReadWrite,
}

impl Access {
    /// The 3-bit `AP` encoding for this access permission.
    const fn ap_bits(self) -> u32 {
        match self {
            Access::NoAccess => 0b000,
            Access::ReadWrite => 0b011,
            Access::ReadOnly => 0b110,
        }
    }
}

/// A computed MPU region descriptor — the `RBAR`/`RASR` register pair.
///
/// Pure bit-packing with no hardware side effects, so it is fully testable on
/// the host. Apply it to the MPU with [`Mpu::configure`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RegionConfig {
    /// Region Base Address Register value (base | VALID | region number).
    pub rbar: u32,
    /// Region Attribute and Size Register value (size | AP | XN | enable).
    pub rasr: u32,
}

impl RegionConfig {
    /// Build a region descriptor.
    ///
    /// - `region` — MPU region number (0..=7 on a typical ARMv7-M MPU).
    /// - `base` — region base address; must be aligned to `size_bytes`.
    /// - `size_bytes` — region size, a power of two ≥ 32.
    /// - `access` — access permission ([`Access`]).
    /// - `execute_never` — set the `XN` bit (data regions should be `true`).
    ///
    /// A misaligned base or an invalid size is a configuration fault and routes
    /// to [`qf::fusa::on_error`] (crash-only) rather than programming a bogus
    /// region.
    pub fn new(region: u8, base: u32, size_bytes: u32, access: Access, execute_never: bool) -> Self {
        // SIZE field: region size is 2^(SIZE+1), so SIZE = log2(size) - 1, valid
        // for sizes 32 B (SIZE=4) up to 4 GB (SIZE=31).
        if !size_bytes.is_power_of_two() || size_bytes < 32 {
            qf::fusa::on_error(module_path!(), line!());
        }
        let size_field = size_bytes.trailing_zeros() - 1;
        // Base must be aligned to the region size.
        if base & (size_bytes - 1) != 0 {
            qf::fusa::on_error(module_path!(), line!());
        }

        let rbar = (base & 0xFFFF_FFE0) | (1 << 4) /* VALID */ | (region as u32 & 0xF);
        let rasr = (1 /* ENABLE */)
            | (size_field << 1)
            | (access.ap_bits() << 24)
            | ((execute_never as u32) << 28);
        Self { rbar, rasr }
    }

    /// A 32-byte **no-access** guard region at `stack_limit` (the low address of
    /// a descending stack). A stack overflow that touches it faults.
    ///
    /// `stack_limit` must be 32-byte aligned (the natural alignment for the
    /// smallest MPU region).
    pub fn stack_guard(region: u8, stack_limit: u32) -> Self {
        Self::new(region, stack_limit, 32, Access::NoAccess, true)
    }

    /// A read-only, execute-never region (for `.rodata` / `const` state tables).
    pub fn read_only(region: u8, base: u32, size_bytes: u32) -> Self {
        Self::new(region, base, size_bytes, Access::ReadOnly, true)
    }
}

/// Zero-sized handle to the ARMv7-M MPU.
///
/// All methods are `hw`-only register writes; on the host they are absent so the
/// crate still compiles and the [`RegionConfig`] encoding can be unit-tested.
#[cfg(feature = "hw")]
pub struct Mpu;

#[cfg(feature = "hw")]
impl Mpu {
    /// Program `regions` into the MPU, enable MemManage faults, then enable the
    /// MPU (with the default memory map as a background region for privileged
    /// accesses, so unconfigured RAM/flash keeps working).
    ///
    /// # Safety
    ///
    /// Programs the system MPU; the caller must pass non-overlapping, correctly
    /// sized regions and ensure the running code's own code/stack remain
    /// accessible. Call once during start-up before enabling task switching.
    pub unsafe fn configure(regions: &[RegionConfig]) {
        // Disable the MPU while reprogramming.
        core::ptr::write_volatile(MPU_CTRL, 0);
        for r in regions {
            // Select the region via RBAR's VALID+REGION fields, then attributes.
            core::ptr::write_volatile(MPU_RBAR, r.rbar);
            core::ptr::write_volatile(MPU_RASR, r.rasr);
        }
        // Enable the MemManage fault so guard-region hits are caught (else they
        // escalate to HardFault).
        let shcsr = core::ptr::read_volatile(SCB_SHCSR);
        core::ptr::write_volatile(SCB_SHCSR, shcsr | (1 << 16) /* MEMFAULTENA */);
        // Enable MPU | PRIVDEFENA (background region for privileged code).
        core::ptr::write_volatile(MPU_CTRL, 0b101);
        // Ensure the new configuration is in effect before returning.
        cortex_m_barrier();
    }

    /// Disable the MPU (clears `MPU_CTRL.ENABLE`).
    ///
    /// # Safety
    /// Removes spatial protection; use only during controlled shutdown/reconfig.
    pub unsafe fn disable() {
        core::ptr::write_volatile(MPU_CTRL, 0);
        cortex_m_barrier();
    }

    /// Select region `n` without reprogramming it (writes `MPU_RNR`).
    ///
    /// # Safety
    /// Raw register write; affects subsequent `RBAR`/`RASR` accesses.
    pub unsafe fn select_region(n: u8) {
        core::ptr::write_volatile(MPU_RNR, n as u32);
    }
}

/// MemManage exception handler — an MPU access violation (e.g. a stack-guard
/// hit or a write to a read-only state table) takes the crash-only path.
///
/// On a real target the vector table must route `MemManage_Handler` here. It
/// never returns: [`qf::fusa::on_error`] records the fault location and halts,
/// so a spatial fault becomes a deterministic safe-stop instead of silent
/// corruption.
#[cfg(feature = "hw")]
#[no_mangle]
pub extern "C" fn MemManage_Handler() -> ! {
    qf::fusa::on_error(module_path!(), line!())
}

/// DSB + ISB so MPU reconfiguration takes effect before later instructions.
#[cfg(feature = "hw")]
#[inline]
fn cortex_m_barrier() {
    // SAFETY: barrier instructions have no operands and no memory side effects.
    unsafe {
        core::arch::asm!("dsb", "isb", options(nostack, preserves_flags));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ap_bits_match_armv7m_encoding() {
        assert_eq!(Access::NoAccess.ap_bits(), 0b000);
        assert_eq!(Access::ReadWrite.ap_bits(), 0b011);
        assert_eq!(Access::ReadOnly.ap_bits(), 0b110);
    }

    #[test]
    fn region_size_field_and_enable() {
        // 1 KiB region → SIZE = log2(1024) - 1 = 9.
        let r = RegionConfig::new(0, 0x2000_0000, 1024, Access::ReadWrite, false);
        assert_eq!(r.rasr & 1, 1, "ENABLE set");
        assert_eq!((r.rasr >> 1) & 0x1F, 9, "SIZE field");
        assert_eq!((r.rasr >> 24) & 0x7, 0b011, "AP = RW");
        assert_eq!((r.rasr >> 28) & 1, 0, "XN clear");
    }

    #[test]
    fn rbar_carries_base_valid_and_region() {
        let r = RegionConfig::new(3, 0x2000_0400, 1024, Access::ReadWrite, false);
        assert_eq!(r.rbar & 0xF, 3, "region number");
        assert_eq!((r.rbar >> 4) & 1, 1, "VALID set");
        assert_eq!(r.rbar & 0xFFFF_FFE0, 0x2000_0400, "base address");
    }

    #[test]
    fn stack_guard_is_32b_no_access_xn() {
        let g = RegionConfig::stack_guard(5, 0x2000_8000);
        assert_eq!((g.rasr >> 1) & 0x1F, 4, "32-byte region → SIZE 4");
        assert_eq!((g.rasr >> 24) & 0x7, 0b000, "no access");
        assert_eq!((g.rasr >> 28) & 1, 1, "execute-never");
    }

    #[test]
    fn read_only_region_is_ro_xn() {
        let r = RegionConfig::read_only(1, 0x0800_0000, 0x1_0000);
        assert_eq!((r.rasr >> 24) & 0x7, 0b110, "AP = RO");
        assert_eq!((r.rasr >> 28) & 1, 1, "execute-never");
    }
}
