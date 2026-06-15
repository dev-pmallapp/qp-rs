//
// STM32WL_FlashCtrl.cs — STM32WLE5 FLASH controller model with the
// option-byte / RDP enforcement surface needed by Phase 5.4 of the
// security-impl plan (security_impl.md §B9, Renode parity plan §5.4).
//
// Replaces the stock MTD.STM32F4_FlashController for the swm-rs WLE5
// devkit so the firmware's RDP-enable sequence at provisioning end
// can be exercised in Renode without bricking real silicon.
//
// Register map (RM0461 §3.7):
//   +0x00 FLASH_ACR    access control / wait states
//   +0x08 FLASH_KEYR   unlock key (write 0x45670123, then 0xCDEF89AB)
//   +0x0C FLASH_OPTKEYR option-byte key (0x08192A3B, then 0x4C5D6E7F)
//   +0x10 FLASH_SR     status
//   +0x14 FLASH_CR     control (LOCK, OPTLOCK, OPTSTRT, OBL_LAUNCH)
//   +0x18 FLASH_ECCR   ECC (passthrough)
//   +0x20 FLASH_OPTR   option register (carries the RDP field)
//
// Behaviours modelled:
//
//   - KEYR / OPTKEYR unlock state machines clear CR.LOCK / CR.OPTLOCK
//     when the published two-word sequence is observed.  Any other
//     sequence latches SR.PGSERR and re-locks CR.LOCK (matching the
//     silicon's "wrong key = re-lock" trap).
//   - OPTR writes are dropped silently while CR.OPTLOCK is set.
//     Writes are accepted (but staged) when unlocked; the staged value
//     becomes the live OPTR only when CR.OPTSTRT fires.
//   - OBL_LAUNCH triggers a "soft reset" of the controller state and
//     copies the staged OPTR into the live one — modelling the reset
//     that real silicon performs.
//   - RDP level (bits [7:0] of OPTR) is decoded as 0xAA = Level 0,
//     0xCC = Level 2, anything else = Level 1.  The current level is
//     exposed via the magic RDP_LEVEL register so Robot suites can
//     assert "firmware committed the right level" without inferring
//     it from the option-byte word.
//
// Magic test-surface registers (out-of-spec):
//   +0x3F0 RDP_LEVEL     read-only: 0, 1, or 2.
//   +0x3F4 OPTSTRT_HITS  number of times CR.OPTSTRT has fired since reset.
//   +0x3F8 BAD_KEYS      number of failed unlock-key sequences observed.
//

using System;
using Antmicro.Renode.Core;
using Antmicro.Renode.Logging;
using Antmicro.Renode.Peripherals.Bus;

namespace Antmicro.Renode.Peripherals.Miscellaneous
{
    [AllowedTranslations(AllowedTranslation.ByteToDoubleWord | AllowedTranslation.WordToDoubleWord)]
    public class STM32WL_FlashCtrl : IDoubleWordPeripheral, IKnownSize
    {
        public STM32WL_FlashCtrl(IMachine machine)
        {
            this.machine = machine;
            Reset();
        }

        public long Size => 0x400;

        public void Reset()
        {
            acr   = 0u;
            sr    = 0u;
            cr    = CR_LOCK | CR_OPTLOCK; // locked at reset
            ecc   = 0u;
            optr  = DEFAULT_OPTR;
            stagedOptr  = DEFAULT_OPTR;
            keyrState   = 0;
            optkeyrState = 0;
            optstrtHits = 0;
            badKeys     = 0;
        }

        public uint ReadDoubleWord(long offset)
        {
            switch(offset)
            {
                case ACR_OFFSET:     return acr;
                case KEYR_OFFSET:    return 0u; // KEYR is write-only
                case OPTKEYR_OFFSET: return 0u;
                case SR_OFFSET:      return sr;
                case CR_OFFSET:      return cr;
                case ECCR_OFFSET:    return ecc;
                case OPTR_OFFSET:    return optr;
                case RDP_LEVEL_OFFSET:    return CurrentRdpLevel();
                case OPTSTRT_HITS_OFFSET: return optstrtHits;
                case BAD_KEYS_OFFSET:     return badKeys;
                default:             return 0u;
            }
        }

        public void WriteDoubleWord(long offset, uint value)
        {
            switch(offset)
            {
                case ACR_OFFSET:
                    acr = value;
                    break;
                case KEYR_OFFSET:
                    HandleKeyrWrite(value);
                    break;
                case OPTKEYR_OFFSET:
                    HandleOptkeyrWrite(value);
                    break;
                case SR_OFFSET:
                    // Write-1-to-clear bits in SR; mask only what we model.
                    sr &= ~(value & (SR_EOP | SR_PGSERR | SR_OPERR));
                    break;
                case CR_OFFSET:
                    HandleCrWrite(value);
                    break;
                case ECCR_OFFSET:
                    ecc = value;
                    break;
                case OPTR_OFFSET:
                    if((cr & CR_OPTLOCK) == 0)
                    {
                        stagedOptr = value;
                    }
                    else
                    {
                        this.Log(LogLevel.Warning, "STM32WL_FlashCtrl: FLASH_OPTR write while OPTLOCK set — dropped");
                    }
                    break;
                default:
                    break;
            }
        }

        private void HandleKeyrWrite(uint value)
        {
            switch(keyrState)
            {
                case 0 when value == KEYR_KEY1:
                    keyrState = 1;
                    break;
                case 1 when value == KEYR_KEY2:
                    cr &= ~CR_LOCK;
                    keyrState = 0;
                    break;
                default:
                    // Wrong key — silicon re-locks and latches PGSERR.
                    cr |= CR_LOCK;
                    sr |= SR_PGSERR;
                    keyrState = 0;
                    badKeys++;
                    break;
            }
        }

        private void HandleOptkeyrWrite(uint value)
        {
            // OPTKEYR also requires CR.LOCK already cleared.
            if((cr & CR_LOCK) != 0)
            {
                sr |= SR_PGSERR;
                badKeys++;
                optkeyrState = 0;
                return;
            }
            switch(optkeyrState)
            {
                case 0 when value == OPTKEYR_KEY1:
                    optkeyrState = 1;
                    break;
                case 1 when value == OPTKEYR_KEY2:
                    cr &= ~CR_OPTLOCK;
                    optkeyrState = 0;
                    break;
                default:
                    cr |= CR_OPTLOCK;
                    sr |= SR_PGSERR;
                    optkeyrState = 0;
                    badKeys++;
                    break;
            }
        }

        private void HandleCrWrite(uint value)
        {
            // LOCK and OPTLOCK are sticky-when-set: software can re-lock
            // but not unlock through CR (must go via *KEYR sequences).
            var newCr = value;
            if((cr & CR_LOCK) != 0)     newCr |= CR_LOCK;
            if((cr & CR_OPTLOCK) != 0)  newCr |= CR_OPTLOCK;
            cr = newCr;

            if((value & CR_OPTSTRT) != 0 && (cr & CR_OPTLOCK) == 0)
            {
                CommitStagedOptr();
            }
            if((value & CR_OBL_LAUNCH) != 0)
            {
                // OBL_LAUNCH reloads option bytes — for the model this is
                // equivalent to committing the staged value and re-locking
                // CR.LOCK / CR.OPTLOCK (silicon resets the whole MCU).
                CommitStagedOptr();
                cr |= CR_LOCK | CR_OPTLOCK;
            }
        }

        private void CommitStagedOptr()
        {
            optr = stagedOptr;
            optstrtHits++;
            sr |= SR_EOP;
            // Live OPTR change immediately re-arms OPTLOCK on silicon.
            cr |= CR_OPTLOCK;
            this.Log(LogLevel.Info, "STM32WL_FlashCtrl: FLASH_OPTR committed = 0x{0:X8} (RDP level {1})", optr, CurrentRdpLevel());
        }

        private uint CurrentRdpLevel()
        {
            var rdp = (byte)(optr & 0xFFu);
            if(rdp == RDP_LEVEL0_PATTERN) return 0u;
            if(rdp == RDP_LEVEL2_PATTERN) return 2u;
            return 1u;
        }

        // ── State ─────────────────────────────────────────────────────────
        private readonly IMachine machine;
        private uint acr;
        private uint sr;
        private uint cr;
        private uint ecc;
        private uint optr;
        private uint stagedOptr;
        private uint keyrState;
        private uint optkeyrState;
        private uint optstrtHits;
        private uint badKeys;

        // ── Register offsets ─────────────────────────────────────────────
        private const long ACR_OFFSET     = 0x00;
        private const long KEYR_OFFSET    = 0x08;
        private const long OPTKEYR_OFFSET = 0x0C;
        private const long SR_OFFSET      = 0x10;
        private const long CR_OFFSET      = 0x14;
        private const long ECCR_OFFSET    = 0x18;
        private const long OPTR_OFFSET    = 0x20;
        private const long RDP_LEVEL_OFFSET    = 0x3F0;
        private const long OPTSTRT_HITS_OFFSET = 0x3F4;
        private const long BAD_KEYS_OFFSET     = 0x3F8;

        // ── Constants ────────────────────────────────────────────────────
        // Default OPTR for the WLE5: RDP=0xAA (Level 0), other fields per
        // RM0461 §3.7.8 reset value.  We zero the non-RDP fields since
        // the model only enforces RDP semantics.
        private const uint DEFAULT_OPTR = 0x000000AAu;
        private const uint KEYR_KEY1    = 0x45670123u;
        private const uint KEYR_KEY2    = 0xCDEF89ABu;
        private const uint OPTKEYR_KEY1 = 0x08192A3Bu;
        private const uint OPTKEYR_KEY2 = 0x4C5D6E7Fu;
        private const byte RDP_LEVEL0_PATTERN = 0xAA;
        private const byte RDP_LEVEL2_PATTERN = 0xCC;

        private const uint CR_LOCK        = 1u << 31;
        private const uint CR_OPTLOCK     = 1u << 30;
        private const uint CR_OPTSTRT     = 1u << 17;
        private const uint CR_OBL_LAUNCH  = 1u << 27;

        private const uint SR_EOP    = 1u << 0;
        private const uint SR_OPERR  = 1u << 1;
        private const uint SR_PGSERR = 1u << 7;
    }
}
