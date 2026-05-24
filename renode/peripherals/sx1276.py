# SX1276 LoRa transceiver — Renode Python peripheral model.
#
# Implements the SPI register interface of the SX1276 (LoRa mode only).
# Accumulates SPI bytes, tracks the FIFO write sequence, and dumps the full
# LoRaWAN frame to the Renode log when the firmware sets TX mode (0x83).
#
# Attach in a .repl platform file:
#   sx1276: Python.PythonPeripheral @ spi2
#       filename: @peripherals/sx1276.py
#       size: 0x80
#
# SX1276 SPI protocol:
#   First byte: addr[6:0] | R/W (bit7=1 write, bit7=0 read)
#   Subsequent bytes: data

# ── Register addresses ────────────────────────────────────────────────────────

REG_FIFO                = 0x00
REG_OP_MODE             = 0x01
REG_FR_MSB              = 0x06
REG_FR_MID              = 0x07
REG_FR_LSB              = 0x08
REG_PA_CONFIG           = 0x09
REG_FIFO_ADDR_PTR       = 0x0D
REG_FIFO_TX_BASE_ADDR   = 0x0E
REG_FIFO_RX_BASE_ADDR   = 0x0F
REG_IRQ_FLAGS           = 0x12
REG_MODEM_CONFIG1       = 0x1D
REG_MODEM_CONFIG2       = 0x1E
REG_PAYLOAD_LENGTH      = 0x22
REG_VERSION             = 0x42

OP_MODE_TX = 0x83  # LoRa + TX mode
VERSION_ID = 0x12

# ── Peripheral state ──────────────────────────────────────────────────────────

regs        = bytearray(0x80)
fifo        = bytearray(256)
spi_addr    = 0
spi_write   = False
spi_byte_n  = 0  # byte index within current SPI transaction
tx_pending  = False

regs[REG_VERSION] = VERSION_ID
regs[REG_FIFO_TX_BASE_ADDR] = 0x00
regs[REG_FIFO_RX_BASE_ADDR] = 0x80

# ── Renode peripheral callbacks ───────────────────────────────────────────────

def get_size():
    return 0x80

def receive(byte):
    global spi_addr, spi_write, spi_byte_n, tx_pending

    if spi_byte_n == 0:
        spi_write = bool(byte & 0x80)
        spi_addr  = byte & 0x7F
        spi_byte_n = 1
        return 0x00

    result = 0x00

    if spi_write:
        if spi_addr == REG_FIFO:
            fifo_ptr = regs[REG_FIFO_ADDR_PTR]
            fifo[fifo_ptr] = byte
            regs[REG_FIFO_ADDR_PTR] = (fifo_ptr + 1) & 0xFF
        elif spi_addr == REG_OP_MODE:
            regs[spi_addr] = byte
            if byte == OP_MODE_TX:
                tx_pending = True
        else:
            regs[spi_addr] = byte
            spi_addr = (spi_addr + 1) & 0x7F
    else:
        result = regs[spi_addr]
        spi_addr = (spi_addr + 1) & 0x7F

    spi_byte_n += 1
    return result

def finish_transmission():
    global spi_byte_n, tx_pending

    spi_byte_n = 0

    if tx_pending:
        tx_pending = False
        payload_len = regs[REG_PAYLOAD_LENGTH]
        base_addr   = regs[REG_FIFO_TX_BASE_ADDR]
        frame       = bytes(fifo[base_addr:base_addr + payload_len])

        self.Log(LogLevel.Info,
            "SX1276 TX frame [{} B]: {}".format(
                payload_len,
                " ".join("{:02X}".format(b) for b in frame)
            )
        )
        _decode_lorawan(frame)

def _decode_lorawan(frame):
    if len(frame) < 8:
        self.Log(LogLevel.Warning, "SX1276: frame too short to decode")
        return

    mhdr = frame[0]
    mtype = (mhdr >> 5) & 0x07
    mtype_names = {2: "UnconfUp", 4: "ConfUp"}
    mtype_str = mtype_names.get(mtype, "MType={}".format(mtype))

    dev_addr = int.from_bytes(frame[1:5], "little")
    fctrl    = frame[5]
    fcnt     = int.from_bytes(frame[6:8], "little")
    fopts_len = fctrl & 0x0F

    hdr_end  = 8 + fopts_len
    has_port = len(frame) > hdr_end + 4
    fport    = frame[hdr_end] if has_port else None
    payload  = frame[hdr_end + 1:-4] if has_port else b""
    mic      = frame[-4:]

    self.Log(LogLevel.Info,
        "SX1276 LoRaWAN {} DevAddr={:#010X} FCnt={} FPort={} payload={}B MIC={}".format(
            mtype_str, dev_addr, fcnt,
            fport if fport is not None else "-",
            len(payload),
            mic.hex().upper()
        )
    )
