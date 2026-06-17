# STM32 Flash NVM Audit (Phase 2.7)

Status: Audit complete
Date: 2026-06-03
Scope: Verify Renode flash region addresses against linker memory.x and
`swm-hal::SealedNvm` regions for the STM32WLE5JC and STM32G0B1CETx
ports.

## Flash sizing — Renode vs linker

| Target       | Linker (`memory.x`)                            | Renode SoC `.repl`                                  | Status |
|--------------|------------------------------------------------|-----------------------------------------------------|--------|
| STM32WLE5JC  | `FLASH @ 0x0800_0000 LENGTH = 256K`            | `flash @ 0x08000000 size: 0x40000` (256 KB)         | OK     |
| STM32G0B1CETx| `FLASH @ 0x0800_0000 LENGTH = 512K` (dual bank)| `flash @ 0x08000000 size: 0x80000` (512 KB, contig.)| OK     |
| STM32WLE5JC  | `RAM   @ 0x2000_0000 LENGTH = 64K` (SRAM1+2)   | `sram  @ 0x20000000 size: 0x10000` (64 KB)          | OK     |
| STM32G0B1CETx| `RAM   @ 0x2000_0000 LENGTH = 80K` (SRAM1 only)| `ram   @ 0x20000000 size: 0x24000` (144 KB total)   | Note 1 |

**Note 1:** the G0B1 repl reserves the full 144 KB SRAM range (SRAM1 +
SRAM2 + SRAM3) even though the current linker only exposes SRAM1 to
the firmware. This is intentional — the Renode model allows future
firmware revisions to extend `RAM LENGTH` without touching the
platform description.

## Three-slot fail-safe / main layout

STM32WLE5JC `memory.x` comment:
- FailSafe: `0x0800_0000 .. 0x0800_7FFF` (32 KB, PCROP-protected)
- Main:     `0x0800_8000 .. 0x0803_FFFF` (224 KB)
- New (candidate): external W25Q64JV / MX25R6435F (SPI1 NOR)

Renode coverage today:
- `flash_ctrl: MTD.STM32F4_FlashController @ sysbus 0x58004000` is
  registered in `stm32wle5.repl` and points at the in-SoC flash. The
  controller model does not enforce PCROP or option-byte write-protect
  on the FailSafe sub-region — see Phase 5 task 5.4 for that work.
- External NOR flash on SPI1 is not modelled (the SPI1 block is the
  stock `SPI.STM32SPI` controller; no SPI NOR child peripheral is
  attached). For multi-slot FOTA tests that need the external NOR
  path, a `Micron_MT25Q`-class SPI flash child will need to be wired
  here. Tracked as Phase 2.x follow-up / Phase 3 prerequisite.

## `SealedNvm` impl status

`swm-hal::SealedNvm` defines a region-scoped read/write/erase contract
(`crates/swm-hal/src/lib.rs:556`). There is **no STM32 port
implementation** today — neither `ports/stm32wle5/src/` nor
`ports/stm32g0b1/src/` ship a `SealedNvm` impl. The trait is consumed
by `swm-app::provisioning`; the STM32 firmware bins will not exercise
that path until the impls land.

When the impls are written, the audit re-checks:
1. The region base addresses they claim sit inside the `flash` block
   declared in the SoC repl (0x0800_0000..0x0803_FFFF for WLE5;
   0x0800_0000..0x0807_FFFF for G0B1).
2. The region sizes are erase-page aligned for the target
   (`STM32F4_FlashController` defaults to 2 KB pages on WL / G0).
3. The PCROP-protected FailSafe sub-region is excluded from any
   `SealedNvm` region the firmware writes at runtime (FailSafe slot is
   provisioning-time only).

## Decision

No code changes required for Phase 2.7. The Renode platform's flash
sizing matches both linker scripts. SealedNvm correctness will be
verified again when port impls are added (Phase 3 prerequisite).

PCROP / option-byte enforcement is explicitly out of scope for Phase 2
— it lives in Phase 5 (5.4 — RDP / option-byte enforcement).

External NOR modelling for FOTA is also out of scope here; it is a
Phase 3 / 4 task depending on how the role-agnostic bins handle their
'new' image staging.
