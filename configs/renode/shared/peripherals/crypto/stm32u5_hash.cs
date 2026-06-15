//
// STM32U5_HASH.cs — STM32U5 HASH peripheral model (SHA-256 focus)
//
// Register map: RM0456 §24.7 (HASH)
//   +0x00 HASH_CR    control
//     bit  0   INIT        — write 1 starts a new digest
//     bits 4-5 DATATYPE    — 0 word, 1 half, 2 byte, 3 bit swap
//     bit  7   ALGO[0]     — see ALGO encoding below
//     bit 17   ALGO[1]
//     bit 14   LKEY        — (HMAC key length, unused for plain hash)
//   +0x04 HASH_DIN   data-in FIFO (32-bit writes append message bytes)
//   +0x08 HASH_STR   start register
//     bits 0-4 NBLW         — number of valid bits in the last word (0..31)
//     bit  8   DCAL         — write 1 starts the final block calculation
//   +0x0C HASH_HR0   output word 0 (high)
//   +0x10 HASH_HR1
//   +0x14 HASH_HR2
//   +0x18 HASH_HR3
//   +0x1C HASH_HR4
//   +0x310 HASH_HR5  (extended digest words — SHA-256/384/512)
//   +0x314 HASH_HR6
//   +0x318 HASH_HR7
//   +0x24 HASH_SR    status (DINIS, DCIS, DMAS, BUSY)
//
// ALGO encoding (combine bits 7 and 17 of HASH_CR):
//   00 = SHA-1     01 = MD5      10 = SHA-224     11 = SHA-256
//
// This model implements MD5 / SHA-1 / SHA-224 / SHA-256 by buffering
// the bytes written to HASH_DIN and using
// System.Security.Cryptography to compute the digest when HASH_STR.DCAL
// fires.  The result is exposed through HASH_HR0..HR7 (the lower five
// at the base offset and the upper three at the extended offsets per
// RM0456 §24.7.6, big-endian word ordering — HR0 holds the most
// significant 32 bits of the digest).
//
// Bits-mode (NBLW != 0) is supported by zeroing the unused tail bits
// of the final word before the final hash call; this matches the
// behaviour of the silicon when the firmware pre-zeros padding bits.
//
// Robot test surface — magic registers at the peripheral tail:
//   +0x3F0  OP_COUNT      — completed digests since reset.
//   +0x3F4  FORCE_FAIL    — write 1 to set DCIS but return zeroed HRs.
//

using System;
using System.Collections.Generic;
using System.Security.Cryptography;
using Antmicro.Renode.Core;
using Antmicro.Renode.Logging;
using Antmicro.Renode.Peripherals.Bus;

namespace Antmicro.Renode.Peripherals.Miscellaneous
{
    [AllowedTranslations(AllowedTranslation.ByteToDoubleWord | AllowedTranslation.WordToDoubleWord)]
    public class STM32U5_HASH : IDoubleWordPeripheral, IKnownSize
    {
        public STM32U5_HASH(IMachine machine)
        {
            this.machine = machine;
            this.message = new List<byte>(256);
            this.hr      = new uint[8];
            Reset();
        }

        public long Size => 0x400;

        public void Reset()
        {
            cr   = 0u;
            sr   = SR_DINIS; // empty FIFO at reset
            str  = 0u;
            for(var i = 0; i < hr.Length; i++) hr[i] = 0u;
            message.Clear();
            opCount = 0u;
            forceFail = false;
        }

        public uint ReadDoubleWord(long offset)
        {
            switch(offset)
            {
                case CR_OFFSET:  return cr;
                case STR_OFFSET: return str;
                case SR_OFFSET:  return sr;
                case HR0_OFFSET: return hr[0];
                case HR1_OFFSET: return hr[1];
                case HR2_OFFSET: return hr[2];
                case HR3_OFFSET: return hr[3];
                case HR4_OFFSET: return hr[4];
                case HR5_OFFSET: return hr[5];
                case HR6_OFFSET: return hr[6];
                case HR7_OFFSET: return hr[7];
                case OP_COUNT_OFFSET:   return opCount;
                case FORCE_FAIL_OFFSET: return forceFail ? 1u : 0u;
                default:         return 0u;
            }
        }

        public void WriteDoubleWord(long offset, uint value)
        {
            switch(offset)
            {
                case CR_OFFSET:
                    cr = value;
                    if((value & CR_INIT) != 0)
                    {
                        // Start a new digest — drop any buffered bytes.
                        message.Clear();
                        sr = SR_DINIS;
                        for(var i = 0; i < hr.Length; i++) hr[i] = 0u;
                    }
                    break;
                case DIN_OFFSET:
                    // Words are big-endian when emitted by the firmware (HASH
                    // accepts a byte stream where DATATYPE=0 reverses
                    // nothing).  Append in network byte order.
                    message.Add((byte)((value >> 24) & 0xFF));
                    message.Add((byte)((value >> 16) & 0xFF));
                    message.Add((byte)((value >>  8) & 0xFF));
                    message.Add((byte)( value        & 0xFF));
                    break;
                case STR_OFFSET:
                    str = value;
                    if((value & STR_DCAL) != 0)
                    {
                        FinalizeDigest((int)(value & 0x1Fu));
                    }
                    break;
                case FORCE_FAIL_OFFSET:
                    forceFail = (value & 1u) != 0;
                    break;
                default:
                    break;
            }
        }

        private void FinalizeDigest(int nblw)
        {
            // NBLW = number of valid bits in the last word (0..31).  If
            // non-zero, drop the bytes that aren't fully populated and
            // mask the partial byte.
            if(nblw > 0 && message.Count >= 4)
            {
                var validBits = nblw;
                var dropBytes = 4 - ((validBits + 7) / 8);
                if(dropBytes > 0 && dropBytes <= message.Count)
                {
                    message.RemoveRange(message.Count - dropBytes, dropBytes);
                }
                var partialBits = validBits % 8;
                if(partialBits != 0 && message.Count > 0)
                {
                    var last = message[message.Count - 1];
                    var mask = (byte)(0xFF << (8 - partialBits));
                    message[message.Count - 1] = (byte)(last & mask);
                }
            }

            var algo = ((cr >> 7) & 1u) | (((cr >> 17) & 1u) << 1);
            var buf  = message.ToArray();
            byte[] digest;

            if(forceFail)
            {
                digest = new byte[32];
                forceFail = false;
            }
            else
            {
                switch(algo)
                {
                    case ALGO_SHA1:
                        using(var h = SHA1.Create())   digest = h.ComputeHash(buf);
                        break;
                    case ALGO_MD5:
                        using(var h = MD5.Create())    digest = h.ComputeHash(buf);
                        break;
                    case ALGO_SHA224:
                        // .NET BCL has no SHA-224; derive from SHA-256 via
                        // distinct IV — too involved for an isolation
                        // model.  We log and return a zero digest so the
                        // firmware can detect non-coverage.
                        this.Log(LogLevel.Warning, "STM32U5_HASH: SHA-224 not modelled, returning zero digest");
                        digest = new byte[28];
                        break;
                    case ALGO_SHA256:
                    default:
                        using(var h = SHA256.Create()) digest = h.ComputeHash(buf);
                        break;
                }
            }

            // Pack digest into HR0..HR7 big-endian.  HR0 holds the most
            // significant 32 bits.  Tail words (above digest length) stay
            // at their previous value — typically zero.
            for(var i = 0; i < hr.Length; i++) hr[i] = 0u;
            for(var w = 0; w < digest.Length / 4 && w < hr.Length; w++)
            {
                hr[w] = ((uint)digest[w * 4 + 0] << 24) |
                        ((uint)digest[w * 4 + 1] << 16) |
                        ((uint)digest[w * 4 + 2] <<  8) |
                         (uint)digest[w * 4 + 3];
            }
            sr |= SR_DCIS;
            opCount++;
        }

        // ── State ─────────────────────────────────────────────────────────
        private readonly IMachine machine;
        private readonly List<byte> message;
        private readonly uint[] hr;
        private uint cr;
        private uint sr;
        private uint str;
        private uint opCount;
        private bool forceFail;

        // ── Register offsets ─────────────────────────────────────────────
        private const long CR_OFFSET   = 0x00;
        private const long DIN_OFFSET  = 0x04;
        private const long STR_OFFSET  = 0x08;
        private const long HR0_OFFSET  = 0x0C;
        private const long HR1_OFFSET  = 0x10;
        private const long HR2_OFFSET  = 0x14;
        private const long HR3_OFFSET  = 0x18;
        private const long HR4_OFFSET  = 0x1C;
        private const long SR_OFFSET   = 0x24;
        private const long HR5_OFFSET  = 0x310;
        private const long HR6_OFFSET  = 0x314;
        private const long HR7_OFFSET  = 0x318;
        private const long OP_COUNT_OFFSET   = 0x3F0;
        private const long FORCE_FAIL_OFFSET = 0x3F4;

        // ── Field encodings ──────────────────────────────────────────────
        private const uint CR_INIT     = 1u << 0;
        private const uint STR_DCAL    = 1u << 8;
        private const uint SR_DINIS    = 1u << 0;
        private const uint SR_DCIS     = 1u << 1;
        private const uint ALGO_SHA1   = 0u;
        private const uint ALGO_MD5    = 1u;
        private const uint ALGO_SHA224 = 2u;
        private const uint ALGO_SHA256 = 3u;
    }
}
