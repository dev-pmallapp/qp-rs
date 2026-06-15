# -*- coding: utf-8 -*-
#
# Pressure sensor I2C stub via STM32 I2C v2 registers.
#
# STM32 variant of `pressure_i2c_stub.py` (which decodes ESP32-C6 I2C
# controller layout).  Same 6-byte payload encoding for a BMP388 /
# DPS310-class sensor:
#   bytes[0..2] = raw pressure ADC count (24-bit little-endian, 1 LSB = 1 Pa)
#   bytes[3..5] = raw temperature ADC count (24-bit little-endian, 1 LSB = 0.01 °C)
#
# STM32 I2C v2 register layout (RM0461 §29 / RM0444 §28; shared across
# G0 / L4 / WL):
#   +0x00  CR1     — accepted, ignored
#   +0x04  CR2     — START / STOP / NBYTES / RD_WRN; accepted, ignored
#   +0x18  ISR     — read returns RXNE | TXIS | TC | TCR | STOPF
#                    so embassy-stm32 I2C polls don't spin
#   +0x1C  ICR     — write clears flags; accepted, ignored
#   +0x24  RXDR    — read returns next byte from the 6-byte canned payload
#   +0x28  TXDR    — write accepted, ignored (firmware sends the BMP388
#                    register pointer; stub always returns the same 6
#                    bytes)
#   all other offsets: writes ignored, reads return 0.
#
# Magic write registers — peripheral-isolation tests / Robot keywords:
#   +0x3F0  pressure_pa (default 98000 = 980.00 hPa)
#   +0x3F4  temp_c100   (default 2500  =  25.00 °C × 100)
# Offsets are <0x400 so the stub fits in one STM32 I2C register window.

DEFAULT_PRESSURE_PA = 98000
DEFAULT_TEMP_C100   = 2500


# Silence linter warnings about Renode-injected globals.
if "request" not in globals():
    try:
        import sys
        import os
        sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        from renode_swm.common import *  # noqa: E402
    except ImportError:
        class MockRequest(object):
            IsInit = False
            IsWrite = False
            IsRead = False
            Offset = 0
            Value = 0
        request = MockRequest()

        class MockSelf(object):
            def Log(self, level, msg):
                pass
        self = MockSelf()


class PressureI2cStm32(object):
    def __init__(self):
        self.reset_state()

    def reset_state(self):
        self.pressure_pa = DEFAULT_PRESSURE_PA
        self.temp_c100   = DEFAULT_TEMP_C100
        self._fifo_pos   = 0

    def _raw_bytes(self):
        p = self.pressure_pa & 0xFFFFFF
        t = self.temp_c100   & 0xFFFFFF
        return [
            p & 0xFF, (p >> 8) & 0xFF, (p >> 16) & 0xFF,
            t & 0xFF, (t >> 8) & 0xFF, (t >> 16) & 0xFF,
        ]

    def handle_write(self, offset, value):
        if offset == 0x3F0:
            self.pressure_pa = int(value)
            self._fifo_pos   = 0
        elif offset == 0x3F4:
            self.temp_c100 = int(value)
            self._fifo_pos = 0
        # CR1 / CR2 / OAR / TIMINGR / TXDR / ICR: accepted, ignored.

    def handle_read(self, offset):
        # ISR — flags consulted by embassy-stm32 I2C v2 driver.
        #   bit 0 TXE  — TX buffer empty (= ready for TXDR write)
        #   bit 1 TXIS — TX interrupt status / next byte requested
        #   bit 2 RXNE — RX not empty (= byte available in RXDR)
        #   bit 5 STOPF — stop condition detected
        #   bit 6 TC   — transfer complete
        #   bit 7 TCR  — transfer complete reload
        if offset == 0x18:
            return (1 << 0) | (1 << 1) | (1 << 2) | (1 << 6) | (1 << 7)
        # RXDR — return next byte, cycling through the 6-byte payload.
        if offset == 0x24:
            data = self._raw_bytes()
            b = data[self._fifo_pos % 6]
            self._fifo_pos = (self._fifo_pos + 1) % 6
            return b
        if offset == 0x3F0:
            return self.pressure_pa
        if offset == 0x3F4:
            return self.temp_c100
        return 0


if "_pressure_i2c_stm32" not in globals():
    _pressure_i2c_stm32 = PressureI2cStm32()

if request.IsInit:
    _pressure_i2c_stm32.reset_state()
elif request.IsWrite:
    _pressure_i2c_stm32.handle_write(request.Offset, request.Value)
elif request.IsRead:
    request.Value = _pressure_i2c_stm32.handle_read(request.Offset)
