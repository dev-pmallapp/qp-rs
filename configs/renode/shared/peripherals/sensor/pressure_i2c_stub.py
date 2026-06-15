# -*- coding: utf-8 -*-
# ESP32-C6 I2C controller stub — BMP388/DPS310-class pressure sensor
#
# Base: 0x60004000  Size: 0x2000
#
# Simulates the ESP32-C6 I2C peripheral register interface so that esp-hal
# I2C transactions complete without hanging.  Returns 6 raw bytes:
#   bytes[0..2] = raw pressure ADC count (24-bit little-endian, 1 LSB = 1 Pa)
#   bytes[3..5] = raw temperature ADC count (24-bit little-endian, 1 LSB = 0.01 °C)
#
# Update the raw byte encoding once the real sensor IC and driver are chosen;
# these placeholder values serve firmware bring-up before the driver exists.
#
# ESP32-C6 I2C controller key register offsets (TRM v1.2, Ch. 26):
#   +0x08  I2C_SR_REG       bit [4]=BUS_BUSY, bits[13:8]=RXFIFO_CNT
#   +0x1C  I2C_FIFO_DATA    read → next RX FIFO byte
#   +0x20  I2C_INT_RAW      bit [7]=TRANS_COMPLETE_INT_RAW
#   +0x58..+0x74  I2C_COMD0..7  bit [14]=DONE
#
# Magic write registers (outside real HW range):
#   +0xFF0  pressure_pa  (default 98000 = 980.00 hPa)
#   +0xFF4  temp_c100    (default 2500  =  25.00 °C × 100)

DEFAULT_PRESSURE_PA = 98000  # 980 hPa sea-level nominal
DEFAULT_TEMP_C100   = 2500   # 25.00 °C

class PressureI2cStub:
    def __init__(self):
        self.reset_state()

    def reset_state(self):
        self.pressure_pa = DEFAULT_PRESSURE_PA
        self.temp_c100   = DEFAULT_TEMP_C100
        self._fifo_pos   = 0

    def _raw_bytes(self):
        p = self.pressure_pa & 0xFFFFFF
        t = self.temp_c100  & 0xFFFFFF
        return [
            p & 0xFF, (p >> 8) & 0xFF, (p >> 16) & 0xFF,
            t & 0xFF, (t >> 8) & 0xFF, (t >> 16) & 0xFF,
        ]

    def handle_write(self, offset, value):
        if offset == 0xFF0:
            self.pressure_pa = int(value)
            self._fifo_pos = 0
        elif offset == 0xFF4:
            self.temp_c100 = int(value)
            self._fifo_pos = 0

    def handle_read(self, offset):
        # SR: BUS_BUSY=0, RXFIFO_CNT=6 (6 bytes ready)
        if offset == 0x08:
            return (6 << 8)
        # FIFO_DATA: return next byte, cycling through the 6-byte payload
        if offset == 0x1C:
            data = self._raw_bytes()
            b = data[self._fifo_pos % 6]
            self._fifo_pos = (self._fifo_pos + 1) % 6
            return b
        # INT_RAW: TRANS_COMPLETE always set
        if offset == 0x20:
            return (1 << 7)
        # COMD0-7: all report DONE
        if 0x58 <= offset <= 0x74:
            return (1 << 14)
        # Magic readback
        if offset == 0xFF0:
            return self.pressure_pa
        if offset == 0xFF4:
            return self.temp_c100
        return 0

if "_pressure_i2c" not in globals():
    _pressure_i2c = PressureI2cStub()

if request.IsInit:
    _pressure_i2c.reset_state()
elif request.IsWrite:
    _pressure_i2c.handle_write(request.Offset, request.Value)
elif request.IsRead:
    request.Value = _pressure_i2c.handle_read(request.Offset)
