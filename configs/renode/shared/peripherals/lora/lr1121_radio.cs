//
// LR1121Radio.cs — Semtech LR1121 LoRa transceiver Renode model
//
// Implements IDoubleWordPeripheral (ESP32-C6 GPSPI2 register interface)
// and IRadio (Renode wireless medium connector).
//
// Register layout mirrors GPSPI2 (base address from .repl):
//   +0x000  GPSPI_CMD_REG   — bit 24 (USR) triggers a SPI transfer
//   +0x01C  GPSPI_MS_DLEN_REG — bitlen - 1 for the next transfer
//   +0x098..+0x0D4  W0..W15 — 16 × 4-byte data windows (esp32c6 PAC v0.23.2)
//
// Each call to spi.write() / spi.transfer_in_place() in the firmware
// produces one CMD_REG trigger.  The table below shows how many triggers
// each LR1121 command requires (per crates/swm-hal/src/lora.rs):
//
//   Op              Triggers  State transitions
//   WriteBuffer     3         WbOffset → WbPayload → Idle
//   SetTx           2         Idle (action on T1) → Idle (T2 no-op)
//   SetRx           2         Idle (no-op T1) → Idle (T2 no-op)
//   GetIrqStatus    3         IrqDummy → IrqResp → Idle
//   ClearIrqStatus  2         ClearIrqMask → Idle
//   GetRxBufStatus  3         RxBufDummy → RxBufResp → Idle
//   ReadBuffer      4         ReadBufOffset → ReadBufDummy → ReadBufData → Idle
//
// IRQ bit assignments (match firmware polling in swm-hal::lora::lr1121):
//   Bit 3 (0x08): TxDone
//   Bit 1 (0x02): RxDone
//
// GetIrqStatus W0 encoding: W0 = ReverseBytes(irqFlags) so that
//   u32::from_be_bytes(irq_buf) == irqFlags in the firmware.
//

using System;
using System.Collections.Generic;
using Antmicro.Renode.Core;
using Antmicro.Renode.Logging;
using Antmicro.Renode.Peripherals.Bus;
using Antmicro.Renode.Peripherals.Wireless;

namespace Antmicro.Renode.Peripherals.SPI
{
    [AllowedTranslations(AllowedTranslation.ByteToDoubleWord | AllowedTranslation.WordToDoubleWord)]
    public class LR1121Radio : IDoubleWordPeripheral, IRadio, IKnownSize
    {
        public LR1121Radio(IMachine machine)
        {
            this.machine = machine;
            wRegisters   = new uint[16];
            txBuffer     = new List<byte>(64);
            rxBuffer     = Array.Empty<byte>();
            rxQueue      = new Queue<byte[]>();
        }

        // ── IKnownSize ────────────────────────────────────────────────────
        public long Size => 0x1000;

        // ── IRadio ────────────────────────────────────────────────────────
        public event Action<IRadio, byte[]> FrameSent;

        public int Channel { get; set; }

        public void ReceiveFrame(byte[] frame, IRadio sender)
        {
            lock (sync)
            {
                // Enqueue rather than overwrite, so back-to-back peer
                // transmissions (e.g. FOTA chunks, the pairing handshake)
                // are all observable. The head is promoted into rxBuffer
                // when the firmware drains the previous frame.
                rxQueue.Enqueue(frame);
                if (rxBuffer.Length == 0 && rxQueue.Count > 0)
                {
                    rxBuffer = rxQueue.Dequeue();
                }
                irqFlags |= IRQ_RXDONE;
            }
            this.Log(LogLevel.Info, "LR1121 RxDone: received {0} bytes (queue depth={1})", frame.Length, rxQueue.Count);
        }

        // ── IDoubleWordPeripheral ─────────────────────────────────────────
        public uint ReadDoubleWord(long offset)
        {
            lock (sync)
            {
                if (offset >= W_BASE && offset <= W_END)
                    return wRegisters[(offset - W_BASE) / 4];
                if (offset == TXCOUNT_OFFSET)
                    return txCount;  // STS §6.9 vehicle — see Phase 3.5.1
                return 0u;  // CMD_REG reads 0 → USR cleared → transaction done
            }
        }

        public void WriteDoubleWord(long offset, uint value)
        {
            lock (sync)
            {
                if (offset >= W_BASE && offset <= W_END)
                {
                    wRegisters[(offset - W_BASE) / 4] = value;
                    return;
                }
                if (offset == DLEN_OFFSET)
                {
                    // GPSPI_MS_DLEN_REG: bits[17:0] = bitlen - 1
                    lastByteLen = (int)((value & 0x3FFFFu) + 1 + 7) / 8;
                    return;
                }
                if (offset == FORCE_TX_FAIL_OFFSET)
                {
                    // STS §6.6 vehicle — see Phase 3.5.6. Robot writes
                    // a non-zero count of upcoming SetTx ops that should
                    // be silently dropped (no FrameSent, no IRQ_TXDONE);
                    // the firmware's poll-for-TxDone loop in
                    // crates/swm-drivers/src/lora.rs times out and the
                    // EspLoraComms::send path prints `SWM TX failed`.
                    forceFailCount = value;
                    return;
                }
                if (offset == CMD_OFFSET && (value & CMD_USR_BIT) != 0)
                    ProcessTransaction();
            }
        }

        public void Reset()
        {
            lock (sync)
            {
                Array.Clear(wRegisters, 0, wRegisters.Length);
                txBuffer.Clear();
                rxBuffer   = Array.Empty<byte>();
                rxQueue.Clear();
                irqFlags        = 0u;
                pendingIrq      = 0u;
                readOffset      = 0;
                txCount         = 0u;
                forceFailCount  = 0u;
                state           = State.Idle;
            }
        }

        // ── Known LR1121 command opcodes used by the firmware driver.
        //     During WbPayload / ReadBufData state we may be mid-way through a
        //     multi-transaction payload transfer (esp-hal chunks SPI writes >64
        //     bytes into 64-byte bursts while CS stays low). If the next
        //     transaction's first 2 bytes match one of these opcodes AND the
        //     transaction length is opcode-sized (≤16 bytes), we treat it as a
        //     new command and fall out of the payload state. Otherwise we keep
        //     accumulating payload — this lets the model deliver the full
        //     ≥128-byte FOTA chunks without truncating to the 64-byte W-window.
        private static bool IsKnownOpcode(ushort cmd)
        {
            return cmd == 0x0109 || cmd == 0x020A || cmd == 0x020B
                || cmd == 0x0114 || cmd == 0x0115 || cmd == 0x010D
                || cmd == 0x0108 || cmd == 0x0201 || cmd == 0x0207
                || cmd == 0x011B || cmd == 0x0210;
        }

        // ── Transaction dispatcher ────────────────────────────────────────
        private void ProcessTransaction()
        {
            byte[] bytes   = GetDataBytes();
            var    cmdCode = (ushort)((bytes[0] << 8) | bytes[1]);

            switch (state)
            {
                // ── WriteBuffer (0x0109): T2 — offset byte ───────────────
                case State.WbOffset:
                    // bytes[0] is the buffer offset (always 0 in firmware); skip it.
                    state = State.WbPayload;
                    return;

                // ── WriteBuffer: T3+ — payload data (may span multiple
                //     SPI transactions when payload > 64 bytes). The only
                //     command the firmware issues directly after a WriteBuffer
                //     is SetTx (0x020A, exactly 2 opcode bytes); recognise
                //     that as the end-of-payload signal and dispatch it. Any
                //     wider opcode heuristic mis-matches LoRa header bytes —
                //     e.g. version=1+kind=PairRequest=9 happen to encode
                //     `0x0109`, the WriteBuffer opcode.
                case State.WbPayload:
                    if (lastByteLen == 2 && cmdCode == 0x020A)
                    {
                        state = State.Idle;
                        break;
                    }
                    txBuffer.AddRange(GetPayloadBytes());
                    return;

                // ── ClearIrqStatus (0x0115): T2 — mask bytes ─────────────
                case State.ClearIrqMask:
                    // bytes[0..3] carry the big-endian 4-byte clear mask.
                    {
                        uint mask = ((uint)bytes[0] << 24) |
                                    ((uint)bytes[1] << 16) |
                                    ((uint)bytes[2] <<  8) |
                                     (uint)bytes[3];
                        irqFlags &= ~mask;
                    }
                    state = State.Idle;
                    return;

                // ── GetIrqStatus (0x0114): T2 — dummy byte ───────────────
                case State.IrqDummy:
                    state = State.IrqResp;
                    return;

                // ── GetIrqStatus: T3 — response ──────────────────────────
                case State.IrqResp:
                    // Encode 32-bit flags big-endian into W0 so that
                    // u32::from_be_bytes(irq_buf) in the firmware recovers irqFlags.
                    wRegisters[0] = ReverseBytes(pendingIrq);
                    wRegisters[1] = 0u;
                    state = State.Idle;
                    return;

                // ── GetRxBufferStatus (0x010D): T2 — dummy byte ──────────
                case State.RxBufDummy:
                    state = State.RxBufResp;
                    return;

                // ── GetRxBufferStatus: T3 — response ─────────────────────
                case State.RxBufResp:
                    // W0[7:0] = rx_len  W0[15:8] = rx_offset (always 0)
                    wRegisters[0] = (uint)(rxBuffer.Length & 0xFF);
                    state = State.Idle;
                    return;

                // ── ReadBuffer (0x0108): T2 — offset byte ────────────────
                case State.ReadBufOffset:
                    readOffset = bytes[0];
                    state      = State.ReadBufDummy;
                    return;

                // ── ReadBuffer: T3 — dummy byte ──────────────────────────
                case State.ReadBufDummy:
                    state = State.ReadBufData;
                    return;

                // ── ReadBuffer: T4+ — frame data (may span multiple SPI
                //     transactions when rx_len > 64 bytes) ────────────────
                case State.ReadBufData:
                {
                    Array.Clear(wRegisters, 0, wRegisters.Length);
                    int chunkLen = Math.Min(64, Math.Max(0, lastByteLen));
                    int remaining = Math.Max(0, rxBuffer.Length - readOffset);
                    chunkLen = Math.Min(chunkLen, remaining);
                    for (int i = 0; i < chunkLen; i++)
                    {
                        wRegisters[i / 4] |= (uint)rxBuffer[readOffset + i] << ((i % 4) * 8);
                    }
                    readOffset += chunkLen;
                    if (readOffset >= rxBuffer.Length)
                    {
                        // Frame fully drained — promote the next queued frame
                        // (if any) so back-to-back peer sends are observable.
                        rxBuffer   = rxQueue.Count > 0 ? rxQueue.Dequeue() : Array.Empty<byte>();
                        readOffset = 0;
                        state      = State.Idle;
                    }
                    // else: stay in ReadBufData for the next chunk transaction
                    return;
                }
            }

            // ── Idle: dispatch on opcode ──────────────────────────────────
            switch (cmdCode)
            {
                case 0x0109:  // WriteBuffer — T1 of 3; offset and payload follow
                    txBuffer.Clear();
                    state = State.WbOffset;
                    break;

                case 0x020A:  // SetTx — flush TX buffer into the wireless medium
                {
                    byte[] frame = txBuffer.ToArray();
                    txBuffer.Clear();
                    // STS §6.9 vehicle: a per-TX counter exposed via
                    // ReadDoubleWord(TXCOUNT_OFFSET). Robot tests poll this
                    // instead of hooking the firmware-side kick_watchdog
                    // symbol — the CPU-hook path slows the simulation past
                    // the cycle suite's wall-clock budget. See Phase 3.5.1.
                    // The counter increments even when the TX is forced to
                    // fail below — watchdog is upstream of the radio
                    // dispatch, so a failed SetTx still kicked it.
                    txCount++;
                    if (forceFailCount > 0)
                    {
                        // STS §6.6 vehicle — Phase 3.5.6. Skip IRQ_TXDONE
                        // and the FrameSent dispatch so the firmware's
                        // GetIrqStatus poll loop in
                        // crates/swm-drivers/src/lora.rs times out and
                        // EspLoraComms::send returns Err(IoError).
                        forceFailCount--;
                        this.Log(LogLevel.Info, "LR1121 SetTx: forced-fail (remaining={0}, txCount={1})", forceFailCount, txCount);
                    }
                    else
                    {
                        irqFlags |= IRQ_TXDONE;
                        this.Log(LogLevel.Info, "LR1121 TxDone: transmitting {0} bytes (txCount={1})", frame.Length, txCount);
                        FrameSent?.Invoke(this, frame);
                    }
                    break;
                }

                case 0x020B:  // SetRx — enter continuous receive mode.
                    // Do NOT clear rxBuffer/rxQueue here. Entering RX must not
                    // destroy a frame that already arrived via ReceiveFrame():
                    // at boot the peer can transmit before this node finishes
                    // arming RX, so clearing here drops the in-flight frame and
                    // wedges the pairing handshake. Received frames persist in
                    // the FIFO (b163d0f) until ReadBuffer drains them.
                    break;

                case 0x0114:  // GetIrqStatus — T1 of 3
                    pendingIrq = irqFlags;
                    state      = State.IrqDummy;
                    break;

                case 0x0115:  // ClearIrqStatus — T1 of 2; mask comes next
                    state = State.ClearIrqMask;
                    break;

                case 0x010D:  // GetRxBufferStatus — T1 of 3
                    state = State.RxBufDummy;
                    break;

                case 0x0108:  // ReadBuffer — T1 of 4
                    state = State.ReadBufOffset;
                    break;
            }
        }

        // ── Helpers ───────────────────────────────────────────────────────
        private byte[] GetDataBytes()
        {
            var result = new byte[64];
            for (int i = 0; i < 16; i++)
            {
                uint w = wRegisters[i];
                result[i * 4 + 0] = (byte)( w        & 0xFF);
                result[i * 4 + 1] = (byte)((w >>  8) & 0xFF);
                result[i * 4 + 2] = (byte)((w >> 16) & 0xFF);
                result[i * 4 + 3] = (byte)((w >> 24) & 0xFF);
            }
            return result;
        }

        private byte[] GetPayloadBytes()
        {
            int byteLen = Math.Min(64, Math.Max(0, lastByteLen));
            var result = new byte[byteLen];
            for (int i = 0; i < byteLen; i++)
                result[i] = (byte)((wRegisters[i / 4] >> ((i % 4) * 8)) & 0xFF);
            return result;
        }

        private static uint ReverseBytes(uint v)
            => ((v & 0xFF000000u) >> 24) |
               ((v & 0x00FF0000u) >>  8) |
               ((v & 0x0000FF00u) <<  8) |
               ((v & 0x000000FFu) << 24);

        // ── State machine ─────────────────────────────────────────────────
        private enum State
        {
            Idle,
            WbOffset, WbPayload,
            ClearIrqMask,
            IrqDummy, IrqResp,
            RxBufDummy, RxBufResp,
            ReadBufOffset, ReadBufDummy, ReadBufData,
        }

        // ── Fields ────────────────────────────────────────────────────────
        private readonly IMachine   machine;
        private readonly uint[]     wRegisters;
        private readonly List<byte> txBuffer;
        private byte[]  rxBuffer;
        private readonly Queue<byte[]> rxQueue;
        private uint    irqFlags;
        private uint    pendingIrq;
        private int     readOffset;
        private int     lastByteLen = 64;
        private uint    txCount;
        private uint    forceFailCount;
        private State   state = State.Idle;

        private readonly object sync = new object();

        private const long CMD_OFFSET     = 0x000;
        private const long DLEN_OFFSET    = 0x01C;
        private const long W_BASE         = 0x098;
        private const long W_END          = 0x0D4;
        // Virtual register — model-only, outside the real GPSPI2 layout
        // (real ESP32-C6 GPSPI2 has no registers past ~0x0FC). Robot tests
        // read this at the LR1121 SPI base + 0xFF0 to observe TX count
        // without a firmware-side CPU hook.
        private const long TXCOUNT_OFFSET = 0xFF0;
        // Virtual register — model-only. Robot tests write this at SPI base
        // + 0xFF4 with the number of upcoming SetTx ops to silently drop
        // (no IRQ_TXDONE, no FrameSent). Decremented on each consumed fail
        // so a write of N produces exactly N back-to-back firmware-side
        // timeouts before normal TX resumes. STS §6.6 vehicle — see Phase
        // 3.5.6.
        private const long FORCE_TX_FAIL_OFFSET = 0xFF4;
        private const uint CMD_USR_BIT    = 1u << 24;
        private const uint IRQ_TXDONE     = 0x08u;
        private const uint IRQ_RXDONE     = 0x02u;
    }

    // ── STM32 SPI variant ─────────────────────────────────────────────────
    //
    // For STM32 ports the LR1121 is driven over a real STM32SPI register
    // block (byte-stream DR/SR/CR1/CR2) rather than the ESP32 GPSPI2
    // burst-mode register window. The STM32 SPI byte-stream FSM lives in
    // `lr1121_subghz.cs` as `LR1121SubGhzRadio` (used for STM32WL SUBGHZSPI
    // at 0x58010000).
    //
    // `LR1121SpiRadio` re-exports the same model under a name that signals
    // "external LR1121 module on a generic STM32 SPI bus" — used by the
    // STM32G0B1 platform at SPI1 (0x40013000). Behaviour is identical to
    // `LR1121SubGhzRadio`; the alias exists so the platform repls / .resc
    // can pick a type that names the binding context for grep-ability.
    public class LR1121SpiRadio : LR1121SubGhzRadio
    {
        public LR1121SpiRadio(IMachine machine) : base(machine) { }
    }
}
