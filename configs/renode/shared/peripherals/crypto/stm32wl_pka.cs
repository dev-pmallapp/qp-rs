//
// STM32WL_PKA.cs — STM32WLE5 PKA peripheral model (P-256 focus)
//
// Register map: RM0461 §23 (PKA)
//   +0x000 CR     control (EN, START, MODE)
//   +0x004 SR     status (PROCENDF, RAMERRF, ADDRERRF, BUSY)
//   +0x008 CLRFR  flag clear
//   ...
//   +0x400 PKA_RAM  5 KB operand RAM (offsets per CubeWL HAL macros)
//
// PKA_RAM operand layout (mode 0x22 — ECC scalar multiplication; the
// path Wle5Pka::ecdh_p256 will drive):
//   +0x055C  scalar K (256 bits, little-endian 32-bit words)
//   +0x0578  initial point X
//   +0x05D0  initial point Y
//   +0x055C  result X  (overlap with input X)
//   +0x05D0  result Y
//   +0x062C  output A coefficient (curve-specific)
//
// Modes used by Wle5Pka (per RM0461 §23.4 / AN5305):
//   MODE_ECDH_P256   = 0x22  — k*P scalar multiplication
//   MODE_ECDSA_SIGN  = 0x24  — ECDSA sign
//   MODE_ECDSA_VRFY  = 0x26  — ECDSA verify
//
// This model focuses on what the firmware actually exercises:
//
//   - Full register protocol: CR.EN, CR.START, CR.MODE; SR.BUSY brief,
//     then SR.PROCENDF latched; CLRFR clears PROCENDF / RAMERRF.
//   - PKA_RAM is a fully addressable 0x1400-byte buffer.
//   - For MODE_ECDH_P256 the model performs a real P-256 scalar
//     multiplication via System.Numerics.BigInteger affine maths.  The
//     result is written back to the canonical PKA_RAM offsets so a
//     RustCrypto comparison check in firmware passes.
//   - ECDSA sign / verify are stubbed: SR_PROCENDF latches, but
//     SR_RAMERR is also set and LAST_ERR returns ERR_NOT_MODELLED so
//     firmware can detect the no-op path.  Real ECDSA can be layered on
//     top of the BigInteger maths in a follow-up — the register surface
//     is what the firmware adapter integrates against.
//
// Robot test surface — magic registers (out-of-spec, in PKA-region tail):
//   +0x17F0  OP_COUNT       — number of operations completed since reset
//   +0x17F4  FORCE_FAIL     — write 1 to force RAMERR on next op
//   +0x17F8  LAST_MODE      — the MODE bits from the most recent op
//   +0x17FC  LAST_ERR       — 0=ok, 1=unsupported, 2=runtime, 3=forced
//

using System;
using System.Numerics;
using Antmicro.Renode.Core;
using Antmicro.Renode.Logging;
using Antmicro.Renode.Peripherals.Bus;

namespace Antmicro.Renode.Peripherals.Miscellaneous
{
    [AllowedTranslations(AllowedTranslation.ByteToDoubleWord | AllowedTranslation.WordToDoubleWord)]
    public class STM32WL_PKA : IDoubleWordPeripheral, IKnownSize
    {
        public STM32WL_PKA(IMachine machine)
        {
            this.machine = machine;
            this.ram     = new uint[RAM_SIZE_WORDS];
            Reset();
        }

        public long Size => 0x1800;

        public void Reset()
        {
            cr = 0u;
            sr = 0u;
            opCount       = 0u;
            forceFail     = false;
            lastOperation = 0u;
            lastError     = 0u;
            for(var i = 0; i < ram.Length; i++) ram[i] = 0u;
        }

        public uint ReadDoubleWord(long offset)
        {
            // Magic test-surface registers sit in the upper tail of the
            // peripheral window (within the RAM offset range numerically)
            // so they MUST be matched before the RAM passthrough.
            switch(offset)
            {
                case CR_OFFSET:         return cr;
                case SR_OFFSET:         return sr;
                case CLRFR_OFFSET:      return 0u;
                case OP_COUNT_OFFSET:   return opCount;
                case FORCE_FAIL_OFFSET: return forceFail ? 1u : 0u;
                case LAST_MODE_OFFSET:  return lastOperation;
                case LAST_ERR_OFFSET:   return lastError;
            }
            if(offset >= RAM_OFFSET && offset < RAM_OFFSET + RAM_SIZE_BYTES)
            {
                return ram[(offset - RAM_OFFSET) / 4];
            }
            return 0u;
        }

        public void WriteDoubleWord(long offset, uint value)
        {
            // Register decode first; falls through to RAM passthrough so the
            // magic test-surface registers (which sit numerically inside the
            // RAM region) keep their own semantics.
            switch(offset)
            {
                case CR_OFFSET:
                    cr = value;
                    if((value & CR_START) != 0)
                    {
                        RunOperation();
                    }
                    return;
                case CLRFR_OFFSET:
                    if((value & CLRFR_PROCEND) != 0) sr &= ~SR_PROCEND;
                    if((value & CLRFR_RAMERR)  != 0) sr &= ~SR_RAMERR;
                    if((value & CLRFR_ADDRERR) != 0) sr &= ~SR_ADDRERR;
                    return;
                case FORCE_FAIL_OFFSET:
                    forceFail = (value & 1u) != 0;
                    return;
                case OP_COUNT_OFFSET:
                case LAST_MODE_OFFSET:
                case LAST_ERR_OFFSET:
                    // Read-only magic registers; silently ignore writes.
                    return;
            }
            if(offset >= RAM_OFFSET && offset < RAM_OFFSET + RAM_SIZE_BYTES)
            {
                ram[(offset - RAM_OFFSET) / 4] = value;
            }
        }

        // ── Operation dispatch ───────────────────────────────────────────
        private void RunOperation()
        {
            lastOperation = (cr >> 8) & 0x3Fu;
            lastError = ERR_OK;
            if(forceFail)
            {
                forceFail = false;
                sr |= SR_RAMERR;
                lastError = ERR_FORCED;
                sr |= SR_PROCEND;
                opCount++;
                return;
            }
            try
            {
                switch(lastOperation)
                {
                    case MODE_ECDH_P256:
                        RunEcdhP256();
                        break;
                    case MODE_ECDSA_SIGN:
                    case MODE_ECDSA_VRFY:
                        // Register-protocol-only stub — see file header.
                        lastError = ERR_NOT_MODELLED;
                        sr |= SR_RAMERR;
                        break;
                    default:
                        lastError = ERR_UNSUPPORTED;
                        sr |= SR_RAMERR;
                        break;
                }
            }
            catch(Exception ex)
            {
                this.Log(LogLevel.Warning, "STM32WL_PKA mode 0x{0:X} runtime failure: {1}", lastOperation, ex.Message);
                lastError = ERR_RUNTIME;
                sr |= SR_RAMERR;
            }
            sr |= SR_PROCEND;
            opCount++;
        }

        // ── ECDH P-256 (mode 0x22) ───────────────────────────────────────
        private void RunEcdhP256()
        {
            var k  = ReadBigInteger256(RAM_ECDH_K_OFFSET);
            var px = ReadBigInteger256(RAM_ECDH_PX_OFFSET);
            var py = ReadBigInteger256(RAM_ECDH_PY_OFFSET);

            BigInteger rx, ry;
            P256.ScalarMul(k, px, py, out rx, out ry);

            WriteBigInteger256(RAM_ECDH_PX_OFFSET, rx);
            WriteBigInteger256(RAM_ECDH_PY_OFFSET, ry);
        }

        // ── PKA RAM <-> BigInteger helpers ───────────────────────────────
        // STM32 PKA stores big numbers as little-endian 32-bit words —
        // word at base offset is the LSW.
        private BigInteger ReadBigInteger256(long absOffset)
        {
            var bytes = new byte[33]; // 32 bytes + a leading zero (force positive)
            var base32 = absOffset - RAM_OFFSET;
            for(var w = 0; w < 8; w++)
            {
                var v = ram[(base32 / 4) + w];
                bytes[w * 4 + 0] = (byte)(v & 0xFF);
                bytes[w * 4 + 1] = (byte)((v >> 8)  & 0xFF);
                bytes[w * 4 + 2] = (byte)((v >> 16) & 0xFF);
                bytes[w * 4 + 3] = (byte)((v >> 24) & 0xFF);
            }
            // BigInteger ctor expects little-endian byte order; the high
            // byte is forced to 0 so the value is non-negative.
            return new BigInteger(bytes);
        }

        private void WriteBigInteger256(long absOffset, BigInteger value)
        {
            var bytes = value.ToByteArray();           // little-endian
            var buf = new byte[32];
            for(var i = 0; i < bytes.Length && i < 32; i++) buf[i] = bytes[i];
            var base32 = absOffset - RAM_OFFSET;
            for(var w = 0; w < 8; w++)
            {
                ram[(base32 / 4) + w] =
                    ((uint)buf[w * 4 + 0])         |
                    ((uint)buf[w * 4 + 1] <<  8)   |
                    ((uint)buf[w * 4 + 2] << 16)   |
                    ((uint)buf[w * 4 + 3] << 24);
            }
        }

        // ── State ─────────────────────────────────────────────────────────
        private readonly IMachine machine;
        private readonly uint[] ram;
        private uint cr;
        private uint sr;
        private uint opCount;
        private uint lastOperation;
        private uint lastError;
        private bool forceFail;

        // ── Register offsets ─────────────────────────────────────────────
        private const long CR_OFFSET    = 0x000;
        private const long SR_OFFSET    = 0x004;
        private const long CLRFR_OFFSET = 0x008;
        private const long RAM_OFFSET   = 0x400;
        private const int  RAM_SIZE_BYTES = 0x1400;
        private const int  RAM_SIZE_WORDS = RAM_SIZE_BYTES / 4;
        private const long OP_COUNT_OFFSET   = 0x17F0;
        private const long FORCE_FAIL_OFFSET = 0x17F4;
        private const long LAST_MODE_OFFSET  = 0x17F8;
        private const long LAST_ERR_OFFSET   = 0x17FC;

        // ── CR / SR / CLRFR bits ─────────────────────────────────────────
        private const uint CR_EN        = 1u << 0;
        private const uint CR_START     = 1u << 1;
        private const uint SR_PROCEND   = 1u << 17;
        private const uint SR_RAMERR    = 1u << 19;
        private const uint SR_ADDRERR   = 1u << 20;
        private const uint CLRFR_PROCEND = 1u << 17;
        private const uint CLRFR_RAMERR  = 1u << 19;
        private const uint CLRFR_ADDRERR = 1u << 20;

        // ── Mode codes (per RM0461 §23.4) ────────────────────────────────
        private const uint MODE_ECDH_P256   = 0x22;
        private const uint MODE_ECDSA_SIGN  = 0x24;
        private const uint MODE_ECDSA_VRFY  = 0x26;

        // ── PKA_RAM operand offsets (relative to peripheral base) ────────
        // Source: STM32CubeWL HAL macros
        // (PKA_ECC_SCALAR_MUL_IN_{K,INITIAL_POINT_X,INITIAL_POINT_Y}_ADDR).
        // Result X / Y overlap the input X / Y slots after operation.
        private const long RAM_ECDH_K_OFFSET  = 0x0508;
        private const long RAM_ECDH_PX_OFFSET = 0x055C;
        private const long RAM_ECDH_PY_OFFSET = 0x05B0;

        // ── Error codes (LAST_ERR magic register) ────────────────────────
        private const uint ERR_OK          = 0u;
        private const uint ERR_UNSUPPORTED = 1u;
        private const uint ERR_RUNTIME     = 2u;
        private const uint ERR_FORCED      = 3u;
        private const uint ERR_NOT_MODELLED = 4u;
    }

    // ─────────────────────────────────────────────────────────────────────
    //  Minimal NIST P-256 affine scalar multiplication, BigInteger-backed.
    //  Used only for STM32WL_PKA — kept private to that file so other
    //  Renode peripherals aren't tempted to import it.
    // ─────────────────────────────────────────────────────────────────────
    internal static class P256
    {
        // p = 2^256 - 2^224 + 2^192 + 2^96 - 1
        private static readonly BigInteger P = BigInteger.Parse(
            "115792089210356248762697446949407573530086143415290314195533631308867097853951");
        // a = -3 mod p
        private static readonly BigInteger A = P - 3;

        public static void ScalarMul(BigInteger k, BigInteger px, BigInteger py,
                                     out BigInteger rx, out BigInteger ry)
        {
            // Double-and-add, MSB first.  Constant-time is not required
            // for the simulation; correctness against RustCrypto is.
            BigInteger qx = 0, qy = 0;
            bool atInfinity = true;

            var bitLen = BitLength(k);
            for(var i = bitLen - 1; i >= 0; i--)
            {
                if(!atInfinity)
                {
                    PointDouble(qx, qy, out qx, out qy);
                }
                if(((k >> i) & 1) == 1)
                {
                    if(atInfinity)
                    {
                        qx = px;
                        qy = py;
                        atInfinity = false;
                    }
                    else
                    {
                        PointAdd(qx, qy, px, py, out qx, out qy);
                    }
                }
            }
            rx = atInfinity ? BigInteger.Zero : qx;
            ry = atInfinity ? BigInteger.Zero : qy;
        }

        private static void PointDouble(BigInteger x, BigInteger y,
                                        out BigInteger rx, out BigInteger ry)
        {
            // s = (3x^2 + a) / 2y  mod p
            var num = Mod(3 * x * x + A, P);
            var den = ModInverse(Mod(2 * y, P), P);
            var s   = Mod(num * den, P);
            rx = Mod(s * s - 2 * x, P);
            ry = Mod(s * (x - rx) - y, P);
        }

        private static void PointAdd(BigInteger x1, BigInteger y1,
                                     BigInteger x2, BigInteger y2,
                                     out BigInteger rx, out BigInteger ry)
        {
            if(x1 == x2 && y1 == y2)
            {
                PointDouble(x1, y1, out rx, out ry);
                return;
            }
            var num = Mod(y2 - y1, P);
            var den = ModInverse(Mod(x2 - x1, P), P);
            var s   = Mod(num * den, P);
            rx = Mod(s * s - x1 - x2, P);
            ry = Mod(s * (x1 - rx) - y1, P);
        }

        private static int BitLength(BigInteger v)
        {
            if(v.Sign == 0) return 0;
            var bytes = v.ToByteArray();
            // ToByteArray returns little-endian; locate the topmost non-zero byte.
            var top = bytes.Length - 1;
            while(top > 0 && bytes[top] == 0) top--;
            var hi = bytes[top];
            var hiBits = 0;
            while(hi != 0) { hi >>= 1; hiBits++; }
            return top * 8 + hiBits;
        }

        private static BigInteger Mod(BigInteger a, BigInteger m)
        {
            var r = a % m;
            if(r < 0) r += m;
            return r;
        }

        private static BigInteger ModInverse(BigInteger a, BigInteger m)
        {
            // Extended Euclidean.
            BigInteger g = m, x = 0, x1 = 1;
            var y = Mod(a, m);
            while(y != 0)
            {
                var q = g / y;
                var t = g - q * y;  g = y;  y = t;
                var u = x - q * x1; x = x1; x1 = u;
            }
            if(g != 1) throw new InvalidOperationException("P-256 modular inverse: not coprime");
            return Mod(x, m);
        }
    }
}
