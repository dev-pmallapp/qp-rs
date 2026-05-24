# SX1262 LoRa transceiver — Renode Python peripheral model.
#
# Implements the command-based SPI protocol of the SX1262.
# Accumulates WriteBuffer data and dumps the LoRaWAN frame to the Renode log
# when the firmware issues SetTx (0x83).
#
# Attach in a .repl platform file:
#   sx1262: Python.PythonPeripheral @ spi2
#       filename: @peripherals/sx1262.py
#       size: 0x1
#
# SX1262 SPI protocol:
#   Byte 0: opcode
#   Remaining bytes: opcode-specific parameters

# ── SX1262 opcodes ────────────────────────────────────────────────────────────

CMD_GET_STATUS         = 0xC0
CMD_WRITE_REGISTER     = 0x0D
CMD_READ_REGISTER      = 0x1D
CMD_WRITE_BUFFER       = 0x0E
CMD_READ_BUFFER        = 0x1E
CMD_SET_SLEEP          = 0x84
CMD_SET_STANDBY        = 0x80
CMD_SET_FS             = 0xC1
CMD_SET_TX             = 0x83
CMD_SET_RX             = 0x82
CMD_SET_RF_FREQUENCY   = 0x86
CMD_SET_PKT_PARAMS     = 0x8C
CMD_SET_MODULATION     = 0x8B
CMD_SET_TX_PARAMS      = 0x8E
CMD_SET_BUF_BASE_ADDR  = 0x8F
CMD_GET_IRQ_STATUS     = 0x12
CMD_CLR_IRQ_STATUS     = 0x02

# ── Peripheral state ──────────────────────────────────────────────────────────

tx_buf      = bytearray(256)
buf_offset  = 0           # WriteBuffer offset (usually 0)
payload_len = 0           # SetPacketParams header_len param
opcode      = None
cmd_bytes   = []          # accumulated bytes for current command
spi_byte_n  = 0
busy        = False

# ── Renode peripheral callbacks ───────────────────────────────────────────────

def get_size():
    return 0x1

def receive(byte):
    global opcode, cmd_bytes, spi_byte_n, buf_offset, payload_len

    if spi_byte_n == 0:
        opcode    = byte
        cmd_bytes = []
        spi_byte_n = 1
        return 0x00   # status byte (not busy)

    cmd_bytes.append(byte)
    spi_byte_n += 1

    if opcode == CMD_WRITE_BUFFER:
        if len(cmd_bytes) == 1:
            buf_offset = byte   # first param is buffer offset
        else:
            idx = buf_offset + len(cmd_bytes) - 2
            if 0 <= idx < 256:
                tx_buf[idx] = byte

    return 0x00

def finish_transmission():
    global spi_byte_n, opcode, payload_len

    if opcode == CMD_SET_PKT_PARAMS and len(cmd_bytes) >= 6:
        # param[5] is PayloadLength for LoRa
        payload_len = cmd_bytes[5]

    elif opcode == CMD_SET_TX:
        frame = bytes(tx_buf[buf_offset:buf_offset + payload_len])
        self.Log(LogLevel.Info,
            "SX1262 TX frame [{} B]: {}".format(
                payload_len,
                " ".join("{:02X}".format(b) for b in frame)
            )
        )
        _decode_lorawan(frame)

    spi_byte_n = 0
    opcode     = None
    cmd_bytes  = []

def _decode_lorawan(frame):
    if len(frame) < 8:
        self.Log(LogLevel.Warning, "SX1262: frame too short to decode")
        return

    mhdr = frame[0]
    mtype = (mhdr >> 5) & 0x07
    mtype_names = {2: "UnconfUp", 4: "ConfUp"}
    mtype_str = mtype_names.get(mtype, "MType={}".format(mtype))

    dev_addr  = int.from_bytes(frame[1:5], "little")
    fctrl     = frame[5]
    fcnt      = int.from_bytes(frame[6:8], "little")
    fopts_len = fctrl & 0x0F
    hdr_end   = 8 + fopts_len
    has_port  = len(frame) > hdr_end + 4
    fport     = frame[hdr_end] if has_port else None
    payload   = frame[hdr_end + 1:-4] if has_port else b""
    mic       = frame[-4:]

    self.Log(LogLevel.Info,
        "SX1262 LoRaWAN {} DevAddr={:#010X} FCnt={} FPort={} payload={}B MIC={}".format(
            mtype_str, dev_addr, fcnt,
            fport if fport is not None else "-",
            len(payload),
            mic.hex().upper()
        )
    )
