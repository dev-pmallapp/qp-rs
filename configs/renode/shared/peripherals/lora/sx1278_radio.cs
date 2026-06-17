//
// sx1278_radio.cs — Semtech SX1278 LoRa transceiver Renode model
//
// Implements IDoubleWordPeripheral (ESP32-C6 GPSPI2 register interface)
// and IRadio (Renode wireless medium connector).
//
// GPSPI2 register layout (base 0x60081000):
//   +0x000  GPSPI_CMD_REG      — bit 24 (USR) triggers a SPI transfer
//   +0x01C  GPSPI_MS_DLEN_REG  — (bitlen - 1); determines transfer byte count
//   +0x098..+0x0D4  W0..W15    — 16 × 4-byte data window (little-endian packing)
//
// SX1278 SPI protocol — one CMD_USR trigger per firmware spi.write() / spi.transfer_in_place():
//   data[0]   = cmd_byte: bit 7 = 1 (write) / 0 (read), bits[6:0] = register address
//   data[1]   = single write value; for bulk RegFifo writes, data[1..N-1] are payload bytes
//   Response for reads is placed at data[1] (W0 bits [15:8]) before firmware reads W registers.
//
// SX1278 register subset modelled (explicit-header LoRa, single-byte transactions):
//   0x00  RegFifo              — TX accumulator (write) / RX payload source (read)
//   0x01  RegOpMode            — bits[2:0]: 0x01=Standby, 0x03=TX, 0x05/0x06=RxCont/RxSingle
//   0x0D  RegFifoAddrPtr       — FIFO read/write pointer (firmware sets before RX reads)
//   0x0E  RegFifoTxBaseAddr    — TX FIFO base address (default 0x00)
//   0x0F  RegFifoRxBaseAddr    — RX FIFO base address (default 0x00)
//   0x10  RegFifoRxCurrentAddr — start address of last received packet (r/o; mirrors fifoRxBase)
//   0x12  RegIrqFlags          — TxDone(bit3=0x08) / RxDone(bit6=0x40); write-1-to-clear
//   0x13  RegRxNbBytes         — bytes in last received packet (r/o)
//   0x1B  RegPktRssiValue      — RSSI of last received packet, raw (r/o)
//   0x22  RegPayloadLength     — TX payload length; updated to RX frame length on delivery
//   All other addresses silently accepted on write and return 0x00 on read.
//
// TX flow (firmware → model):
//   1. write_reg(0x0D, 0x00)      — FifoAddrPtr = 0
//   2. write_reg(0x00, b) × N     — accumulate N bytes into txBuffer
//   3. write_reg(0x22, N)         — RegPayloadLength = N
//   4. write_reg(0x01, 0x83)      — mode = TX → DispatchTx(): FrameSent + IRQ_TXDONE
//   5. read_reg(0x12) until TxDone(bit3) set, then write_reg(0x12, 0x08) to clear
//
// RX flow (model → firmware):
//   1. ReceiveFrame() (wireless medium) -or- magic-reg injection via +0xFF4/+0xFFC
//      → rxBuffer populated, payloadLen updated, IRQ_RXDONE set
//   2. Firmware reads read_reg(0x22)  → payloadLen (received frame length)
//   3. Firmware reads read_reg(0x0F)  → fifoRxBase (= 0x00)
//   4. Firmware writes write_reg(0x0D, fifoRxBase) → no-op for model
//   5. Firmware reads read_reg(0x00) × payloadLen → pops bytes from rxBuffer
//
// Magic registers (model-only, compatible with sx1278_spi.py Python model):
//   +0xFF0  r → txCount                (Robot: observe TX activity)
//   +0xFF0  w → pktRssi                (Robot: set simulated RSSI byte)
//   +0xFF4  w → stage one RX byte      (Robot: build RX frame byte-by-byte)
//   +0xFF4  r → RxNbBytes              (Robot: confirm last committed frame length)
//   +0xFF8  w → forceFailCount         (Robot: N upcoming TXs silently dropped)
//   +0xFFC  w → commit staged RX frame (Robot: inject frame, set RxDone)
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
    public class SX1278Radio : IDoubleWordPeripheral, IRadio, IKnownSize
    {
        public SX1278Radio(IMachine machine)
        {
            this.machine  = machine;
            wRegisters    = new uint[16];
            txBuffer      = new List<byte>(64);
            rxBuffer      = Array.Empty<byte>();
            rxQueue       = new Queue<byte[]>();
            rxStaging     = new List<byte>(64);
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
                rxQueue.Enqueue(frame);
                PromoteRxHead();
            }
            this.Log(LogLevel.Info, "SX1278 RxDone: {0} bytes (queue depth={1})", frame.Length, rxQueue.Count);
        }

        // ── IDoubleWordPeripheral ─────────────────────────────────────────
        public uint ReadDoubleWord(long offset)
        {
            lock (sync)
            {
                if (offset >= W_BASE && offset <= W_END)
                    return wRegisters[(offset - W_BASE) / 4];
                if (offset == MAGIC_TXCOUNT)
                    return txCount;
                if (offset == MAGIC_RXSTAGE)
                    return (uint)rxBuffer.Length;
                return 0u;
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
                    // bits[17:0] = (bitlen - 1); round up to bytes
                    lastByteLen = (int)((value & 0x3FFFFu) + 1 + 7) / 8;
                    return;
                }
                if (offset == CMD_OFFSET && (value & CMD_USR_BIT) != 0)
                {
                    ProcessTransaction();
                    return;
                }
                // ── Magic registers ──────────────────────────────────────
                switch (offset)
                {
                    case MAGIC_TXCOUNT:  pktRssi = (byte)(value & 0xFF); break;
                    case MAGIC_RXSTAGE:  rxStaging.Add((byte)(value & 0xFF)); break;
                    case MAGIC_FORCEFL:  forceFailCount = value; break;
                    case MAGIC_RXCOMMIT: CommitStagedRx(); break;
                }
            }
        }

        public void Reset()
        {
            lock (sync)
            {
                Array.Clear(wRegisters, 0, wRegisters.Length);
                txBuffer.Clear();
                rxBuffer      = Array.Empty<byte>();
                rxQueue.Clear();
                rxStaging.Clear();
                irqFlags      = 0u;
                fifoAddrPtr   = 0;
                fifoTxBase    = 0;
                fifoRxBase    = 0;
                readOffset    = 0;
                payloadLen    = 0;
                pktRssi       = 0x5E;   // -65 dBm default
                lastByteLen   = 2;
                txCount       = 0u;
                forceFailCount = 0u;
            }
        }

        // ── SPI transaction dispatcher ────────────────────────────────────
        private void ProcessTransaction()
        {
            // lastByteLen is set by DLEN_REG before each CMD_USR write.
            // At minimum 2 bytes (cmd + one data/dummy byte); cap at W-window.
            int  n      = Math.Min(Math.Max(lastByteLen, 2), 64);
            byte cmd    = GetByte(0);
            bool isWr   = (cmd & 0x80) != 0;
            byte reg    = (byte)(cmd & 0x7F);

            if (isWr)
                ProcessWrite(reg, n);
            else
                ProcessRead(reg);
        }

        private void ProcessWrite(byte reg, int byteLen)
        {
            switch (reg)
            {
                case 0x00:  // RegFifo — accumulate TX payload bytes
                    for (int i = 1; i < byteLen; i++)
                        txBuffer.Add(GetByte(i));
                    break;

                case 0x01:  // RegOpMode — mode[2:0]
                {
                    byte mode = (byte)(GetByte(1) & 0x07);
                    if (mode == 0x03)
                        DispatchTx();
                    // RX modes (0x05 / 0x06): no state change needed; ReceiveFrame()
                    // populates rxBuffer whenever a frame arrives from the medium.
                    break;
                }

                case 0x0D:  fifoAddrPtr = GetByte(1); break;
                case 0x0E:  fifoTxBase  = GetByte(1); break;
                case 0x0F:  fifoRxBase  = GetByte(1); break;

                case 0x12:  // RegIrqFlags — write-1-to-clear
                    irqFlags &= ~(uint)GetByte(1);
                    break;

                case 0x22:  // RegPayloadLength — TX payload byte count
                    payloadLen = GetByte(1);
                    break;

                // All other registers silently accepted (frequency, PA config, modem config, etc.)
            }
        }

        private void ProcessRead(byte reg)
        {
            switch (reg)
            {
                case 0x00:  // RegFifo — pop one byte from rxBuffer per call
                    PlaceByte(1, PopRxByte());
                    break;

                case 0x0D:  PlaceByte(1, fifoAddrPtr); break;
                case 0x0E:  PlaceByte(1, fifoTxBase);  break;
                case 0x0F:  PlaceByte(1, fifoRxBase);  break;
                case 0x10:  PlaceByte(1, fifoRxBase);  break;  // FifoRxCurrentAddr ≡ base
                case 0x12:  PlaceByte(1, (byte)(irqFlags & 0xFF)); break;
                case 0x13:  PlaceByte(1, (byte)(rxBuffer.Length & 0xFF)); break;
                case 0x1B:  PlaceByte(1, pktRssi); break;
                case 0x22:  PlaceByte(1, payloadLen); break;
                default:    PlaceByte(1, 0); break;
            }
        }

        // ── TX dispatch ───────────────────────────────────────────────────
        private void DispatchTx()
        {
            byte[] frame = txBuffer.ToArray();
            txBuffer.Clear();
            txCount++;
            if (forceFailCount > 0)
            {
                forceFailCount--;
                this.Log(LogLevel.Info,
                    "SX1278 SetTx: forced-fail (remaining={0}, txCount={1})",
                    forceFailCount, txCount);
            }
            else
            {
                irqFlags |= IRQ_TXDONE;
                this.Log(LogLevel.Info,
                    "SX1278 TxDone: {0} bytes [{1}] (txCount={2})",
                    frame.Length,
                    BitConverter.ToString(frame).Replace("-", ""),
                    txCount);
                FrameSent?.Invoke(this, frame);
            }
        }

        // ── RX helpers ────────────────────────────────────────────────────
        // Promote the head of rxQueue into rxBuffer and update payloadLen / IRQ.
        private void PromoteRxHead()
        {
            if (rxBuffer.Length == 0 && rxQueue.Count > 0)
            {
                rxBuffer   = rxQueue.Dequeue();
                readOffset = 0;
                payloadLen = (byte)(rxBuffer.Length & 0xFF);
                irqFlags  |= IRQ_RXDONE;
            }
        }

        private byte PopRxByte()
        {
            if (readOffset >= rxBuffer.Length)
                return 0;

            byte b = rxBuffer[readOffset++];
            // Promote the next queued frame as soon as the current one is fully
            // consumed, so payloadLen (reg 0x22) is updated before the firmware's
            // next poll loop reads it — correct even when frame sizes differ.
            if (readOffset >= rxBuffer.Length)
            {
                rxBuffer   = Array.Empty<byte>();
                readOffset = 0;
                if (rxQueue.Count > 0)
                {
                    rxBuffer   = rxQueue.Dequeue();
                    payloadLen = (byte)(rxBuffer.Length & 0xFF);
                    irqFlags  |= IRQ_RXDONE;
                }
            }
            return b;
        }

        private void CommitStagedRx()
        {
            if (rxStaging.Count == 0)
                return;
            byte[] frame = rxStaging.ToArray();
            rxStaging.Clear();
            rxQueue.Enqueue(frame);
            PromoteRxHead();
            this.Log(LogLevel.Info, "SX1278 RX inject: {0} bytes, RxDone set", frame.Length);
        }

        // ── W-register byte helpers ───────────────────────────────────────
        // Byte position i in the flat W-register layout (little-endian packing):
        //   byte 0 = W0[7:0], byte 1 = W0[15:8], byte 4 = W1[7:0], etc.
        private byte GetByte(int i)
            => (byte)((wRegisters[i / 4] >> ((i % 4) * 8)) & 0xFF);

        private void PlaceByte(int i, byte val)
        {
            int  shift = (i % 4) * 8;
            int  wi    = i / 4;
            wRegisters[wi] = (wRegisters[wi] & ~(0xFFu << shift)) | ((uint)val << shift);
        }

        // ── State ─────────────────────────────────────────────────────────
        private readonly IMachine      machine;
        private readonly uint[]        wRegisters;
        private readonly List<byte>    txBuffer;
        private readonly Queue<byte[]> rxQueue;
        private readonly List<byte>    rxStaging;
        private byte[]  rxBuffer;

        private uint irqFlags;
        private byte fifoAddrPtr;
        private byte fifoTxBase;
        private byte fifoRxBase;
        private int  readOffset;
        private byte payloadLen;
        private byte pktRssi       = 0x5E;  // raw PktRssiValue; -65 dBm on HF band
        private int  lastByteLen   = 2;
        private uint txCount;
        private uint forceFailCount;

        private readonly object sync = new object();

        // GPSPI2 offsets (ESP32-C6 PAC v0.23.2)
        private const long CMD_OFFSET  = 0x000;
        private const long DLEN_OFFSET = 0x01C;
        private const long W_BASE      = 0x098;
        private const long W_END       = 0x0D4;

        // Model-only magic registers (compatible with sx1278_spi.py offsets)
        private const long MAGIC_TXCOUNT = 0xFF0;  // r → txCount       w → pktRssi
        private const long MAGIC_RXSTAGE = 0xFF4;  // w → stage byte    r → rxNbBytes
        private const long MAGIC_FORCEFL = 0xFF8;  // w → forceFailCount
        private const long MAGIC_RXCOMMIT = 0xFFC; // w → commit staged RX frame

        private const uint CMD_USR_BIT = 1u << 24;
        private const uint IRQ_TXDONE  = 0x08u;   // RegIrqFlags bit 3
        private const uint IRQ_RXDONE  = 0x40u;   // RegIrqFlags bit 6
    }
}
