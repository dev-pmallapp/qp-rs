//
// LR1121SubGhzRadio.cs — LR1121 transceiver model bound to STM32 SPI
//
// Used at:
//   • STM32WLE5  SUBGHZSPI @ 0x58010000 (integrated sub-GHz radio bus)
//   • STM32G0B1  SPI1      @ 0x40013000 (external LR1121 module)
//
// Both targets expose the same STM32 SPI register layout:
//   +0x00 CR1   — SPE etc.; accepted, mirrored on read.
//   +0x04 CR2   — accepted, ignored.
//   +0x08 SR    — RXNE | TXE | !BSY always asserted so the firmware's
//                 ready-poll loop in `swm-hal::lora` makes forward progress.
//   +0x0C DR    — write enqueues a byte into the LR1121 byte-stream FSM;
//                 read pops one response byte (0x00 when empty).
//
// The LR1121 command set, IRQ flag layout, and txCount/forceTxFail magic
// registers mirror `lr1121_radio.cs` (the ESP32 GPSPI2 variant). The
// state machine is byte-stream oriented rather than register-burst
// oriented, but produces the same observable side-effects so the same
// Robot keywords apply across all three targets.
//
// Magic (out-of-spec) registers — same semantics as lr1121_radio.cs but
// relocated below the 1 KB STM32 SPI block size so the model fits inside
// each port's stock SPI register window (G0B1 USART1 sits at
// SPI1 + 0x800, so the model can't claim 4 KB the way the ESP32 model
// does on GPSPI2).
//   +0x3F0 TXCOUNT      — Robot read; STS §6.9 vehicle.
//   +0x3F4 FORCE_TX_FAIL — Robot write; STS §6.6 vehicle.
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
    public class LR1121SubGhzRadio : IDoubleWordPeripheral, IRadio, IKnownSize
    {
        public LR1121SubGhzRadio(IMachine machine)
        {
            this.machine    = machine;
            txBuffer        = new List<byte>(256);
            rxBuffer        = Array.Empty<byte>();
            rxQueue         = new Queue<byte[]>();
            opcodeBuffer    = new List<byte>(2);
            responseQueue   = new Queue<byte>();
            payloadBuffer   = new List<byte>(256);
            paramBuffer     = new List<byte>(8);
        }

        // ── IKnownSize ────────────────────────────────────────────────────
        public long Size => 0x400;

        // ── IRadio ────────────────────────────────────────────────────────
        public event Action<IRadio, byte[]> FrameSent;

        public int Channel { get; set; }

        public void ReceiveFrame(byte[] frame, IRadio sender)
        {
            lock(sync)
            {
                rxQueue.Enqueue(frame);
                if(rxBuffer.Length == 0 && rxQueue.Count > 0)
                {
                    rxBuffer = rxQueue.Dequeue();
                }
                irqFlags |= IRQ_RXDONE;
            }
            this.Log(LogLevel.Info, "LR1121-SubGhz RxDone: {0} bytes (queue depth={1})", frame.Length, rxQueue.Count);
        }

        // ── IDoubleWordPeripheral ─────────────────────────────────────────
        public uint ReadDoubleWord(long offset)
        {
            lock(sync)
            {
                switch(offset)
                {
                    case CR1_OFFSET:    return cr1;
                    case CR2_OFFSET:    return cr2;
                    case SR_OFFSET:     return SR_TXE | SR_RXNE;
                    case DR_OFFSET:     return PopResponseByte();
                    case TXCOUNT_OFFSET: return txCount;
                    default:            return 0u;
                }
            }
        }

        public void WriteDoubleWord(long offset, uint value)
        {
            lock(sync)
            {
                switch(offset)
                {
                    case CR1_OFFSET:
                        // Detect SPE 1→0 (peripheral disable) as a transaction
                        // terminator — embassy-stm32's SPI driver disables SPE
                        // between transfers so this approximates CS rising.
                        {
                            uint prevSpe = cr1 & CR1_SPE;
                            cr1 = value;
                            if(prevSpe != 0 && (value & CR1_SPE) == 0)
                            {
                                EndTransaction();
                            }
                        }
                        break;
                    case CR2_OFFSET:
                        cr2 = value;
                        break;
                    case DR_OFFSET:
                        PushByte((byte)(value & 0xFF));
                        break;
                    case FORCE_TX_FAIL_OFFSET:
                        forceFailCount = value;
                        break;
                    default:
                        break;
                }
            }
        }

        public void Reset()
        {
            lock(sync)
            {
                txBuffer.Clear();
                rxBuffer    = Array.Empty<byte>();
                rxQueue.Clear();
                opcodeBuffer.Clear();
                responseQueue.Clear();
                payloadBuffer.Clear();
                paramBuffer.Clear();
                irqFlags        = 0u;
                pendingIrq      = 0u;
                readOffset      = 0;
                txCount         = 0u;
                forceFailCount  = 0u;
                cr1             = 0u;
                cr2             = 0u;
                state           = State.Idle;
            }
        }

        // ── Byte-stream FSM ───────────────────────────────────────────────
        // The LR1121 is driven over a real SPI controller — each DR write
        // is one byte. The FSM consumes opcode-then-arguments and produces
        // response bytes that the firmware reads back via DR.
        //
        // Where the ESP32 model relies on per-transaction `lastByteLen` to
        // know when WriteBuffer / ReadBuffer payloads end, this byte-stream
        // model uses two signals:
        //   1. CR1.SPE 1→0 transition (firmware disables SPI between bursts)
        //   2. Heuristic: an opcode-shaped 2-byte sequence at start of
        //      WbPayload terminates the payload and starts the next command.
        //      Mirrors `IsKnownOpcode`-based recovery in lr1121_radio.cs.
        private void PushByte(byte b)
        {
            // Drain mid-response: only feed the FSM with bytes that follow
            // the response stream. The firmware reads dummy 0xFF / 0x00
            // bytes during response reads but those DR-reads come through
            // ReadDoubleWord, not here — so any DR write is genuine command
            // data.

            switch(state)
            {
                case State.Idle:
                    opcodeBuffer.Add(b);
                    if(opcodeBuffer.Count == 2)
                    {
                        ushort opcode = (ushort)((opcodeBuffer[0] << 8) | opcodeBuffer[1]);
                        opcodeBuffer.Clear();
                        DispatchOpcode(opcode);
                    }
                    break;

                case State.WbOffset:
                    // Single offset byte; ignored — firmware always uses 0.
                    state = State.WbPayload;
                    break;

                case State.WbPayload:
                    // Heuristic: payload ends when we recognise an opcode at
                    // start of a new transaction. Without CS-edge signalling
                    // the SPE-toggle approximation handles the common case;
                    // this is the fallback when SPE stays enabled.
                    payloadBuffer.Add(b);
                    break;

                case State.SetTxParam:
                    paramBuffer.Add(b);
                    if(paramBuffer.Count == 3)
                    {
                        paramBuffer.Clear();
                        FlushPendingWriteBuffer();
                        DispatchSetTx();
                        state = State.Idle;
                    }
                    break;

                case State.SetRxParam:
                    paramBuffer.Add(b);
                    if(paramBuffer.Count == 3)
                    {
                        paramBuffer.Clear();
                        DispatchSetRx();
                        state = State.Idle;
                    }
                    break;

                case State.ClearIrqMask:
                    paramBuffer.Add(b);
                    if(paramBuffer.Count == 4)
                    {
                        uint mask = ((uint)paramBuffer[0] << 24)
                                  | ((uint)paramBuffer[1] << 16)
                                  | ((uint)paramBuffer[2] <<  8)
                                  |  (uint)paramBuffer[3];
                        irqFlags &= ~mask;
                        paramBuffer.Clear();
                        state = State.Idle;
                    }
                    break;

                case State.IrqDummy:
                    // Firmware sends one dummy byte after GetIrqStatus opcode
                    // before reading the 4-byte BE flags.
                    pendingIrq = irqFlags;
                    EnqueueU32BE(pendingIrq);
                    state = State.Idle;
                    break;

                case State.RxBufDummy:
                    // GetRxBufferStatus response: [status, rxLen, rxOffset].
                    // Match the ESP32 model: rxLen in byte 0, others zero —
                    // the firmware reads byte 0 for length.
                    responseQueue.Enqueue((byte)(rxBuffer.Length & 0xFF));
                    responseQueue.Enqueue(0);
                    responseQueue.Enqueue(0);
                    state = State.Idle;
                    break;

                case State.ReadBufOffset:
                    readOffset = b;
                    state = State.ReadBufDummy;
                    break;

                case State.ReadBufDummy:
                    // Dummy byte before payload reads; payload bytes are
                    // dispensed by ReadDoubleWord via responseQueue.
                    EnqueueReadBufferData();
                    state = State.ReadBufDispense;
                    break;

                case State.ReadBufDispense:
                    // Firmware drives one DR write per response byte read;
                    // no state change — each DR read pops one byte.
                    break;
            }
        }

        private void DispatchOpcode(ushort opcode)
        {
            switch(opcode)
            {
                case 0x0109:  // WriteBuffer
                    payloadBuffer.Clear();
                    state = State.WbOffset;
                    break;

                case 0x020A:  // SetTx
                    paramBuffer.Clear();
                    state = State.SetTxParam;
                    break;

                case 0x020B:  // SetRx
                    paramBuffer.Clear();
                    state = State.SetRxParam;
                    break;

                case 0x0114:  // GetIrqStatus — expects 1 dummy then 4 response bytes
                    state = State.IrqDummy;
                    break;

                case 0x0115:  // ClearIrqStatus — 4-byte mask
                    paramBuffer.Clear();
                    state = State.ClearIrqMask;
                    break;

                case 0x010D:  // GetRxBufferStatus — 1 dummy then 3 response bytes
                    state = State.RxBufDummy;
                    break;

                case 0x0108:  // ReadBuffer
                    state = State.ReadBufOffset;
                    break;

                default:
                    // Unknown opcode: silently return to Idle. The firmware
                    // either drives a known opcode next or its higher-level
                    // poll loop times out and reports failure — same
                    // observable as lr1121_radio.cs ignoring an unknown
                    // command code.
                    state = State.Idle;
                    break;
            }
        }

        private void DispatchSetTx()
        {
            byte[] frame = txBuffer.ToArray();
            txBuffer.Clear();
            txCount++;
            if(forceFailCount > 0)
            {
                forceFailCount--;
                this.Log(LogLevel.Info, "LR1121-SubGhz SetTx: forced-fail (remaining={0}, txCount={1})", forceFailCount, txCount);
            }
            else
            {
                irqFlags |= IRQ_TXDONE;
                this.Log(LogLevel.Info, "LR1121-SubGhz TxDone: transmitting {0} bytes (txCount={1})", frame.Length, txCount);
                FrameSent?.Invoke(this, frame);
            }
        }

        private void DispatchSetRx()
        {
            rxBuffer = Array.Empty<byte>();
            rxQueue.Clear();
        }

        private void FlushPendingWriteBuffer()
        {
            if(payloadBuffer.Count > 0)
            {
                txBuffer.AddRange(payloadBuffer);
                payloadBuffer.Clear();
            }
        }

        private void EnqueueReadBufferData()
        {
            int remaining = Math.Max(0, rxBuffer.Length - readOffset);
            for(int i = 0; i < remaining; i++)
            {
                responseQueue.Enqueue(rxBuffer[readOffset + i]);
            }
            readOffset += remaining;
            if(readOffset >= rxBuffer.Length)
            {
                rxBuffer   = rxQueue.Count > 0 ? rxQueue.Dequeue() : Array.Empty<byte>();
                readOffset = 0;
            }
        }

        private void EnqueueU32BE(uint value)
        {
            responseQueue.Enqueue((byte)((value >> 24) & 0xFF));
            responseQueue.Enqueue((byte)((value >> 16) & 0xFF));
            responseQueue.Enqueue((byte)((value >>  8) & 0xFF));
            responseQueue.Enqueue((byte)( value        & 0xFF));
        }

        // CR1.SPE 1→0 — treat as CS rising edge: flush any pending state.
        private void EndTransaction()
        {
            if(state == State.WbPayload)
            {
                FlushPendingWriteBuffer();
                state = State.Idle;
            }
            else if(state == State.ReadBufDispense)
            {
                state = State.Idle;
            }
            // Other in-progress states (SetTx param mid-read etc.) stay
            // pending; the firmware will re-enable SPE and continue.
        }

        private uint PopResponseByte()
        {
            return responseQueue.Count > 0 ? responseQueue.Dequeue() : (uint)0;
        }

        // ── State ─────────────────────────────────────────────────────────
        private enum State
        {
            Idle,
            WbOffset, WbPayload,
            SetTxParam, SetRxParam,
            ClearIrqMask,
            IrqDummy,
            RxBufDummy,
            ReadBufOffset, ReadBufDummy, ReadBufDispense,
        }

        private readonly IMachine        machine;
        private readonly List<byte>      txBuffer;
        private readonly Queue<byte[]>   rxQueue;
        private readonly List<byte>      opcodeBuffer;
        private readonly Queue<byte>     responseQueue;
        private readonly List<byte>      payloadBuffer;
        private readonly List<byte>      paramBuffer;

        private byte[] rxBuffer;
        private uint   irqFlags;
        private uint   pendingIrq;
        private int    readOffset;
        private uint   txCount;
        private uint   forceFailCount;
        private uint   cr1;
        private uint   cr2;
        private State  state = State.Idle;

        private readonly object sync = new object();

        // STM32 SPI register layout (RM0444 §32 / RM0461 §30 §38)
        private const long CR1_OFFSET    = 0x00;
        private const long CR2_OFFSET    = 0x04;
        private const long SR_OFFSET     = 0x08;
        private const long DR_OFFSET     = 0x0C;
        // Magic registers — model-only, see lr1121_radio.cs. Offsets are
        // shrunk to fit within Size = 0x400 (STM32 SPI register window).
        private const long TXCOUNT_OFFSET        = 0x3F0;
        private const long FORCE_TX_FAIL_OFFSET  = 0x3F4;

        // STM32 SPI SR bits
        private const uint SR_RXNE = 1u << 0;
        private const uint SR_TXE  = 1u << 1;

        // STM32 SPI CR1 bits
        private const uint CR1_SPE = 1u << 6;

        // LR1121 IRQ bits (same as ESP32 LR1121Radio)
        private const uint IRQ_TXDONE = 0x08u;
        private const uint IRQ_RXDONE = 0x02u;
    }
}
