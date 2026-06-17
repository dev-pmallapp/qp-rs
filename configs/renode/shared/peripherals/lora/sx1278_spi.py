# -*- coding: utf-8 -*-
#
# Semtech SX1278 LoRa Transceiver — Renode SPI2 peripheral model.
#
# Intercepts ESP32-C6 GPSPI2 register reads/writes:
#   GPSPI_W0_REG … GPSPI_W15_REG  (+0x098 … +0x0D4)  — 16×32-bit SPI buffer
#   GPSPI_CMD_REG                  (+0x000), bit 24 (USR) — triggers transfer
#
# SX1278 register subset modelled
# ────────────────────────────────
#   0x00  RegFifo           — TX payload accumulator / RX payload source
#   0x01  RegOpMode         — LongRangeMode + Mode[2:0]
#   0x06  RegFrMsb          — RF carrier frequency MSB
#   0x07  RegFrMid          — RF carrier frequency MID
#   0x08  RegFrLsb          — RF carrier frequency LSB
#   0x09  RegPaConfig       — PA_BOOST / MaxPower / OutputPower
#   0x0D  RegFifoAddrPtr    — FIFO read/write pointer (firmware sets before RX read)
#   0x0E  RegFifoTxBaseAddr — TX FIFO start (default 0x80)
#   0x0F  RegFifoRxBaseAddr — RX FIFO start (default 0x00)
#   0x10  RegFifoRxCurrentAddr — address of last received packet (r/o)
#   0x12  RegIrqFlags       — TxDone(3) / RxDone(6); write-1-to-clear
#   0x13  RegRxNbBytes      — number of bytes in last received packet (r/o)
#   0x1B  RegPktRssiValue   — RSSI of last received packet (r/o)
#   0x1D  RegModemConfig1   — BW / CodingRate / ImplicitHeader
#   0x1E  RegModemConfig2   — SpreadingFactor / TxContinuousMode / RxPayloadCrcOn
#   0x22  RegPayloadLength  — TX payload byte count
#   0x25  RegFifoRxByteAddr — current RX FIFO write address (r/o)
#
# TX flow (firmware → model):
#   1. FifoAddrPtr ← FifoTxBaseAddr
#   2. Write N bytes to RegFifo (0x00)
#   3. RegOpMode ← 0x83 (LoRa + TX)  →  model logs frame, sets TxDone
#
# RX flow (model → firmware):
#   1. Inject frame: write bytes via magic reg 0xFF4 (one byte per write),
#      then write magic reg 0xFFC to commit (sets RxDone + populates FIFO).
#   2. Firmware polls IrqFlags bit 6 (RxDone), then:
#      a. Reads FifoRxCurrentAddr (0x10)
#      b. Writes FifoAddrPtr ← FifoRxCurrentAddr (0x0D)
#      c. Reads RxNbBytes (0x13)
#      d. Reads N bytes from RegFifo (0x00)
#      e. Clears IrqFlags (write 0xFF to 0x12)
#
# Magic registers (simulation control):
#   +0xFF0  w — set simulated packet RSSI (raw SX1278 PktRssiValue byte;
#               e.g. 0x5E = 94 → -65 dBm on HF band via RSSI = -157 + value)
#   +0xFF4  w — push one byte into the RX staging buffer
#   +0xFFC  w — commit staged bytes as a received frame (triggers RxDone)
#   +0xFF0  r — read back current pkt_rssi value
#   +0xFF4  r — read RxNbBytes (number of bytes in last committed frame)
#   +0xFF8  r — read chg_stat_n (0=charging, 1=not-charging) — pass-through
#               so a Robot test can verify the MCP73831T state via LoRa side.

import sys
import os
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from renode_swm import PReg  # noqa: E402

if "request" not in globals():
    try:
        from renode_swm.common import MockRequest, MockSelf, MockAntmicro
    except ImportError:
        class MockRequest(object):
            IsInit = False; IsWrite = False; IsRead = False; Offset = 0; Value = 0
        class MockSelf(object):
            def Log(self, level, msg): pass
        class MockAntmicro(object):
            class Renode(object):
                class Logging(object):
                    class LogLevel(object):
                        Info = 0; Warning = 1; Error = 2
    request  = MockRequest()
    self     = MockSelf()
    Antmicro = MockAntmicro()

# ── IRQ flag bit positions ───────────────────────────────────────────────────
IRQ_TX_DONE = 0x08   # bit 3
IRQ_RX_DONE = 0x40   # bit 6

if request.IsInit:
    wRegisters = [0] * 16

    # SX1278 registers
    op_mode          = PReg(width=8, reset=0x09)  # LoRa Standby (LongRangeMode=1, Mode=001)
    irq_flags        = PReg(width=8, sticky=0xFF)
    fr_msb           = PReg(width=8, reset=0xD9)  # 868.1 MHz default
    fr_mid           = PReg(width=8, reset=0x06)
    fr_lsb           = PReg(width=8, reset=0x66)
    pa_config        = PReg(width=8, reset=0x4F)
    fifo_addr_ptr    = PReg(width=8, reset=0x00)
    fifo_tx_base     = PReg(width=8, reset=0x80)
    fifo_rx_base     = PReg(width=8, reset=0x00)
    fifo_rx_curr     = PReg(width=8, reset=0x00)  # r/o, set on RX injection
    rx_nb_bytes      = PReg(width=8, reset=0x00)  # r/o
    pkt_rssi         = PReg(width=8, reset=0x5E)  # -65 dBm (PktRssiValue = 94)
    modem_cfg1       = PReg(width=8, reset=0x72)  # BW125, CR4/5, explicit header
    modem_cfg2       = PReg(width=8, reset=0x74)  # SF7, CRC on
    payload_len      = PReg(width=8, reset=0x01)
    fifo_rx_byte_ptr = PReg(width=8, reset=0x00)  # r/o

    tx_fifo       = []   # payload bytes being accumulated for TX
    rx_fifo       = []   # payload bytes ready to be read by firmware
    rx_stage_buf  = []   # staging buffer for magic-register RX injection

    def rlog(msg):
        self.Log(Antmicro.Renode.Logging.LogLevel.Info, msg)

elif request.IsWrite:
    off = request.Offset
    val = request.Value

    # ── SPI buffer registers ─────────────────────────────────────────────
    if 0x098 <= off <= 0x0D4:
        wRegisters[(off - 0x098) // 4] = val

    # ── SPI CMD register — USR bit triggers the transfer ─────────────────
    elif off == 0x000:
        if (val & (1 << 24)) == 0:
            pass  # not a USR trigger, ignore
        else:
            # Unpack SPI buffer into a flat byte array
            data = []
            for word in wRegisters:
                data.append(int(word & 0xFF))
                data.append(int((word >> 8)  & 0xFF))
                data.append(int((word >> 16) & 0xFF))
                data.append(int((word >> 24) & 0xFF))

            cmd_byte  = data[0]
            is_wr     = (cmd_byte & 0x80) != 0
            reg_addr  = cmd_byte & 0x7F

            if is_wr:
                # ── write path ──────────────────────────────────────────
                if reg_addr == 0x00:                    # RegFifo write (TX payload)
                    for b in data[1:]:
                        tx_fifo.append(b)

                elif reg_addr == 0x01:                  # RegOpMode
                    op_mode.write(data[1])
                    mode_bits = op_mode.value & 0x07
                    if mode_bits == 0x03:               # TX mode → flush FIFO
                        hex_str = "".join("{:02X}".format(b) for b in tx_fifo)
                        if hex_str:
                            rlog("SX1278: TX frame [{}B] {}".format(len(tx_fifo), hex_str))
                        tx_fifo.clear()
                        irq_flags.set(IRQ_TX_DONE)
                    elif mode_bits in (0x05, 0x06):     # RXCONT / RXSINGLE
                        rlog("SX1278: entering RX mode (OpMode=0x{:02X})".format(op_mode.value))

                elif reg_addr == 0x06:  fr_msb.write(data[1])
                elif reg_addr == 0x07:  fr_mid.write(data[1])
                elif reg_addr == 0x08:  fr_lsb.write(data[1])
                elif reg_addr == 0x09:  pa_config.write(data[1])
                elif reg_addr == 0x0D:  fifo_addr_ptr.write(data[1])
                elif reg_addr == 0x0E:  fifo_tx_base.write(data[1])
                elif reg_addr == 0x0F:  fifo_rx_base.write(data[1])
                elif reg_addr == 0x12:  irq_flags.clear(data[1])  # write-1-to-clear
                elif reg_addr == 0x1D:  modem_cfg1.write(data[1])
                elif reg_addr == 0x1E:  modem_cfg2.write(data[1])
                elif reg_addr == 0x22:  payload_len.write(data[1])

            else:
                # ── read path — place value in byte 1 of wRegisters[0] ─
                def _place(val8):
                    wRegisters[0] = (wRegisters[0] & 0xFF) | ((int(val8) & 0xFF) << 8)

                if reg_addr == 0x00:    # RegFifo read (RX payload)
                    if rx_fifo:
                        # Fill wRegisters bytes 1..N with queued RX bytes
                        for i, b in enumerate(rx_fifo):
                            wi  = (i + 1) // 4
                            shi = ((i + 1) % 4) * 8
                            if wi < 16:
                                wRegisters[wi] = (wRegisters[wi] & ~(0xFF << shi)) | ((b & 0xFF) << shi)
                        rx_fifo.clear()
                    else:
                        _place(0)
                elif reg_addr == 0x01:  _place(op_mode.read())
                elif reg_addr == 0x06:  _place(fr_msb.read())
                elif reg_addr == 0x07:  _place(fr_mid.read())
                elif reg_addr == 0x08:  _place(fr_lsb.read())
                elif reg_addr == 0x09:  _place(pa_config.read())
                elif reg_addr == 0x0D:  _place(fifo_addr_ptr.read())
                elif reg_addr == 0x0E:  _place(fifo_tx_base.read())
                elif reg_addr == 0x0F:  _place(fifo_rx_base.read())
                elif reg_addr == 0x10:  _place(fifo_rx_curr.read())
                elif reg_addr == 0x12:  _place(irq_flags.read())
                elif reg_addr == 0x13:  _place(rx_nb_bytes.read())
                elif reg_addr == 0x1B:  _place(pkt_rssi.read())
                elif reg_addr == 0x1D:  _place(modem_cfg1.read())
                elif reg_addr == 0x1E:  _place(modem_cfg2.read())
                elif reg_addr == 0x22:  _place(payload_len.read())
                elif reg_addr == 0x25:  _place(fifo_rx_byte_ptr.read())
                else:                   _place(0)

    # ── Magic registers ──────────────────────────────────────────────────
    elif off == 0xFF0:
        pkt_rssi.write(int(val) & 0xFF)

    elif off == 0xFF4:
        rx_stage_buf.append(int(val) & 0xFF)

    elif off == 0xFFC:
        # Commit staged bytes as a received frame
        if rx_stage_buf:
            n = len(rx_stage_buf)
            rx_fifo.extend(rx_stage_buf)
            rx_stage_buf.clear()
            fifo_rx_curr.set(fifo_rx_base.read())
            rx_nb_bytes.write(n)
            fifo_rx_byte_ptr.write((fifo_rx_base.read() + n) & 0xFF)
            irq_flags.set(IRQ_RX_DONE)
            rlog("SX1278: RX inject [{}B] frame, RxDone set".format(n))

elif request.IsRead:
    off = request.Offset
    if 0x098 <= off <= 0x0D4:
        request.Value = wRegisters[(off - 0x098) // 4]
    elif off == 0xFF0:
        request.Value = pkt_rssi.read()
    elif off == 0xFF4:
        request.Value = rx_nb_bytes.read()
    else:
        request.Value = 0
