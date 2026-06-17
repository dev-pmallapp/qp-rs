//
// STM32WL_AES1.cs — STM32WLE5 AES1 peripheral model
//
// Register map: RM0461 §22.6 (AES1 register definitions)
//   +0x00 CR     control
//   +0x04 SR     status (CCF, RDERR, WRERR, BUSY)
//   +0x08 DINR   data-in FIFO (4 writes = one 128-bit block)
//   +0x0C DOUTR  data-out FIFO (4 reads = one 128-bit block)
//   +0x10 KEYR0  key word 0  (low)
//   +0x14 KEYR1
//   +0x18 KEYR2
//   +0x1C KEYR3  key word 3  (high)
//   +0x20 IVR0   IV word 0   (CBC / CTR / GCM / CCM)
//   +0x24 IVR1
//   +0x28 IVR2
//   +0x2C IVR3
//   +0x30 KEYR4  upper key words used when CR.KEYSIZE = 1 (256-bit)
//   +0x34 KEYR5
//   +0x38 KEYR6
//   +0x3C KEYR7
//
// CR fields used by this model:
//   bit  0     EN     — enable (block computation when 0)
//   bits 3..4  MODE   — 0 encrypt | 2 decrypt   (1/3 = key-derivation, unused)
//   bits 5..6  CHMOD  — 0 ECB | 1 CBC | 2 CTR    (3 = GCM/GMAC/CCM, not modelled)
//   bit  7     CCFC   — write 1 clears SR.CCF
//   bit 17     KEYSIZE — 0 = 128-bit | 1 = 256-bit
//
// Operation
//   - On the 4th 32-bit write to DINR the model assembles a 16-byte
//     plaintext (or ciphertext, in decrypt mode), runs one block of
//     AES through System.Security.Cryptography.Aes, and queues the
//     result in the DOUTR FIFO.  SR.CCF latches; the firmware clears
//     it via CR.CCFC.
//   - CBC: result is XOR'd with current IV before encryption / after
//     decryption per the standard CBC schedule; IV is then advanced
//     to the produced ciphertext block.
//   - CTR: nonce/counter is the current IV; counter is incremented as
//     a big-endian 128-bit integer after each block.
//
// Robot test surface — magic registers (out of spec, < 0x400 stride):
//   +0x3F0 OP_COUNT      — number of blocks processed since reset.
//   +0x3F4 FORCE_FAIL    — write 1 to force SR.RDERR on the next read.
//
// What this model does NOT cover (yet):
//   - GCM / GMAC / CCM phases (CR.GCMPH).  The firmware's `hw-aes-stm32wl`
//     feature ships an AES-CCM-128 path that will need a follow-up model.
//   - Suspend / resume registers (SUSP0R..7R).
//   - DMA request lines (DMAINEN / DMAOUTEN bits accepted, request lines
//     not asserted).
//
// All inputs are validated against RustCrypto's `aes` crate vectors in
// configs/renode/tests/peripherals/crypto/aes1.robot.
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
    public class STM32WL_AES1 : IDoubleWordPeripheral, IKnownSize
    {
        public STM32WL_AES1(IMachine machine)
        {
            this.machine = machine;
            this.keyWords = new uint[8];
            this.ivWords  = new uint[4];
            this.dinBuffer = new List<uint>(4);
            this.doutFifo  = new Queue<uint>(4);
            Reset();
        }

        public long Size => 0x400;

        public void Reset()
        {
            cr = 0u;
            sr = 0u;
            for(var i = 0; i < keyWords.Length; i++) keyWords[i] = 0u;
            for(var i = 0; i < ivWords.Length;  i++) ivWords[i]  = 0u;
            dinBuffer.Clear();
            doutFifo.Clear();
            opCount = 0u;
            forceFail = false;
        }

        // ── Register IO ───────────────────────────────────────────────────
        public uint ReadDoubleWord(long offset)
        {
            switch(offset)
            {
                case CR_OFFSET:    return cr;
                case SR_OFFSET:    return sr;
                case DOUTR_OFFSET: return PopDoutWord();
                case KEYR0_OFFSET: return keyWords[0];
                case KEYR1_OFFSET: return keyWords[1];
                case KEYR2_OFFSET: return keyWords[2];
                case KEYR3_OFFSET: return keyWords[3];
                case KEYR4_OFFSET: return keyWords[4];
                case KEYR5_OFFSET: return keyWords[5];
                case KEYR6_OFFSET: return keyWords[6];
                case KEYR7_OFFSET: return keyWords[7];
                case IVR0_OFFSET:  return ivWords[0];
                case IVR1_OFFSET:  return ivWords[1];
                case IVR2_OFFSET:  return ivWords[2];
                case IVR3_OFFSET:  return ivWords[3];
                case OP_COUNT_OFFSET:   return opCount;
                case FORCE_FAIL_OFFSET: return forceFail ? 1u : 0u;
                default:           return 0u;
            }
        }

        public void WriteDoubleWord(long offset, uint value)
        {
            switch(offset)
            {
                case CR_OFFSET:
                    cr = value & ~(CR_CCFC | CR_ERRC);
                    if((value & CR_CCFC) != 0) sr &= ~SR_CCF;
                    if((value & CR_ERRC) != 0) sr &= ~(SR_RDERR | SR_WRERR);
                    if((value & CR_EN) == 0)
                    {
                        // Disabling the engine flushes the FIFOs (RM0461 §22.4.3).
                        dinBuffer.Clear();
                        doutFifo.Clear();
                    }
                    break;
                case DINR_OFFSET:
                    if((cr & CR_EN) == 0)
                    {
                        sr |= SR_WRERR;
                        return;
                    }
                    dinBuffer.Add(value);
                    if(dinBuffer.Count == 4)
                    {
                        RunBlock();
                        dinBuffer.Clear();
                    }
                    break;
                case KEYR0_OFFSET: keyWords[0] = value; break;
                case KEYR1_OFFSET: keyWords[1] = value; break;
                case KEYR2_OFFSET: keyWords[2] = value; break;
                case KEYR3_OFFSET: keyWords[3] = value; break;
                case KEYR4_OFFSET: keyWords[4] = value; break;
                case KEYR5_OFFSET: keyWords[5] = value; break;
                case KEYR6_OFFSET: keyWords[6] = value; break;
                case KEYR7_OFFSET: keyWords[7] = value; break;
                case IVR0_OFFSET:  ivWords[0]  = value; break;
                case IVR1_OFFSET:  ivWords[1]  = value; break;
                case IVR2_OFFSET:  ivWords[2]  = value; break;
                case IVR3_OFFSET:  ivWords[3]  = value; break;
                case FORCE_FAIL_OFFSET:
                    forceFail = (value & 1u) != 0;
                    break;
                default:
                    break;
            }
        }

        // ── Block engine ──────────────────────────────────────────────────
        private void RunBlock()
        {
            var mode    = (cr >> 3) & 0x3u;       // 0 enc, 2 dec
            var chmod   = (cr >> 5) & 0x3u;       // 0 ECB, 1 CBC, 2 CTR
            var keysize = ((cr >> 17) & 0x1u) != 0 ? 256 : 128;
            var keyLen  = keysize / 8;
            var key     = new byte[keyLen];
            // RM0461 §22.6.10 / §22.6.13: KEYRn[31:0] holds key bits
            // [(n*32)+31 : n*32], so KEYR3 contains bytes 0..3 of the key
            // (MSB end) for AES-128.  For AES-256 the MSB end is KEYR7.
            // We pack high-index → byte[0], descending.
            var hiKeyIndex = (keyLen / 4) - 1;
            for(var i = 0; i <= hiKeyIndex; i++)
            {
                WriteBE(key, i * 4, keyWords[hiKeyIndex - i]);
            }

            var input = new byte[16];
            // DINR is loaded MSW first (RM0461 §22.4.6): first write is
            // bytes 0..3 of the input block.
            for(var i = 0; i < 4; i++)
            {
                WriteBE(input, i * 4, dinBuffer[i]);
            }

            var output = new byte[16];
            if(chmod == CHMOD_ECB)
            {
                RunEcbBlock(key, input, output, mode == 2);
            }
            else if(chmod == CHMOD_CBC)
            {
                var iv = new byte[16];
                LoadIvIntoBytes(iv);
                if(mode == 2)
                {
                    // CBC decrypt: out = ECB_dec(input) XOR IV; IV := input
                    var raw = new byte[16];
                    RunEcbBlock(key, input, raw, true);
                    for(var i = 0; i < 16; i++) output[i] = (byte)(raw[i] ^ iv[i]);
                    LoadIvFromBytes(input);
                }
                else
                {
                    // CBC encrypt: out = ECB_enc(input XOR IV); IV := out
                    var xored = new byte[16];
                    for(var i = 0; i < 16; i++) xored[i] = (byte)(input[i] ^ iv[i]);
                    RunEcbBlock(key, xored, output, false);
                    LoadIvFromBytes(output);
                }
            }
            else if(chmod == CHMOD_CTR)
            {
                var iv = new byte[16];
                LoadIvIntoBytes(iv);
                var ks = new byte[16];
                RunEcbBlock(key, iv, ks, false);
                for(var i = 0; i < 16; i++) output[i] = (byte)(input[i] ^ ks[i]);
                IncrementCounter(iv);
                LoadIvFromBytes(iv);
            }
            else
            {
                this.Log(LogLevel.Warning, "STM32WL_AES1: CHMOD {0} not implemented; passing input through", chmod);
                Array.Copy(input, output, 16);
            }

            doutFifo.Clear();
            for(var i = 0; i < 4; i++) doutFifo.Enqueue(ReadBE(output, i * 4));
            sr |= SR_CCF;
            opCount++;
        }

        private static void RunEcbBlock(byte[] key, byte[] input, byte[] output, bool decrypt)
        {
            using(var aes = Aes.Create())
            {
                aes.Mode = CipherMode.ECB;
                aes.Padding = PaddingMode.None;
                aes.KeySize = key.Length * 8;
                aes.Key = key;
                aes.IV  = new byte[16];
                using(var tx = decrypt ? aes.CreateDecryptor() : aes.CreateEncryptor())
                {
                    tx.TransformBlock(input, 0, 16, output, 0);
                }
            }
        }

        private uint PopDoutWord()
        {
            if(forceFail)
            {
                sr |= SR_RDERR;
                forceFail = false;
                return 0u;
            }
            if(doutFifo.Count == 0)
            {
                sr |= SR_RDERR;
                return 0u;
            }
            return doutFifo.Dequeue();
        }

        private void LoadIvFromBytes(byte[] iv)
        {
            // IVR3 holds the MSW; iv[0..3] is the MSB end.
            for(var i = 0; i < 4; i++) ivWords[3 - i] = ReadBE(iv, i * 4);
        }

        private void LoadIvIntoBytes(byte[] iv)
        {
            for(var i = 0; i < 4; i++) WriteBE(iv, i * 4, ivWords[3 - i]);
        }

        private static void WriteBE(byte[] buf, int offset, uint value)
        {
            buf[offset + 0] = (byte)((value >> 24) & 0xFF);
            buf[offset + 1] = (byte)((value >> 16) & 0xFF);
            buf[offset + 2] = (byte)((value >>  8) & 0xFF);
            buf[offset + 3] = (byte)( value        & 0xFF);
        }

        private static uint ReadBE(byte[] buf, int offset)
        {
            return ((uint)buf[offset + 0] << 24) |
                   ((uint)buf[offset + 1] << 16) |
                   ((uint)buf[offset + 2] <<  8) |
                    (uint)buf[offset + 3];
        }

        private static void IncrementCounter(byte[] counter)
        {
            for(var i = 15; i >= 0; i--)
            {
                counter[i]++;
                if(counter[i] != 0) break;
            }
        }

        // ── State ─────────────────────────────────────────────────────────
        private readonly IMachine machine;
        private readonly uint[] keyWords;
        private readonly uint[] ivWords;
        private readonly List<uint> dinBuffer;
        private readonly Queue<uint> doutFifo;
        private uint cr;
        private uint sr;
        private uint opCount;
        private bool forceFail;

        // ── Register offsets ─────────────────────────────────────────────
        private const long CR_OFFSET    = 0x00;
        private const long SR_OFFSET    = 0x04;
        private const long DINR_OFFSET  = 0x08;
        private const long DOUTR_OFFSET = 0x0C;
        private const long KEYR0_OFFSET = 0x10;
        private const long KEYR1_OFFSET = 0x14;
        private const long KEYR2_OFFSET = 0x18;
        private const long KEYR3_OFFSET = 0x1C;
        private const long IVR0_OFFSET  = 0x20;
        private const long IVR1_OFFSET  = 0x24;
        private const long IVR2_OFFSET  = 0x28;
        private const long IVR3_OFFSET  = 0x2C;
        private const long KEYR4_OFFSET = 0x30;
        private const long KEYR5_OFFSET = 0x34;
        private const long KEYR6_OFFSET = 0x38;
        private const long KEYR7_OFFSET = 0x3C;
        private const long OP_COUNT_OFFSET   = 0x3F0;
        private const long FORCE_FAIL_OFFSET = 0x3F4;

        // ── CR / SR bits ─────────────────────────────────────────────────
        private const uint CR_EN   = 1u << 0;
        private const uint CR_CCFC = 1u << 7;
        private const uint CR_ERRC = 1u << 8;
        private const uint SR_CCF   = 1u << 0;
        private const uint SR_RDERR = 1u << 1;
        private const uint SR_WRERR = 1u << 2;
        private const uint CHMOD_ECB = 0u;
        private const uint CHMOD_CBC = 1u;
        private const uint CHMOD_CTR = 2u;
    }
}
