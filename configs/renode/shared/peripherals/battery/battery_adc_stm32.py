# -*- coding: utf-8 -*-
#
# Battery + solar ADC simulation via STM32 ADC registers.
#
# STM32 variant of `battery_adc_stub.py` (which decodes ESP32-C6 SAR
# layout).  Same dynamic charge / discharge simulator; only the bus-side
# register decode changes.
#
# STM32 ADC register layout (RM0461 §16 / RM0444 §15; G0 / L4 / WL share
# the same modern ADC IP):
#   +0x00  ISR     — read returns EOC|EOSEQ|ADRDY (always asserted)
#   +0x04  IER     — accepted, ignored
#   +0x08  CR      — ADSTART (bit 2), ADCAL (bit 31); accepted, ignored
#   +0x0C  CFGR1   — accepted, ignored
#   +0x10  CFGR2   — accepted, ignored
#   +0x14  SMPR    — accepted, ignored
#   +0x28  CHSELR  — selected channel mask (1 bit per channel)
#   +0x40  DR      — read returns the converted 12-bit count for the
#                    last-selected channel
#   all other offsets: writes ignored, reads return 0.
#
# Channel mapping (swm-gagan-wle5 board / RM0461 §16.3.10):
#   ADC_IN5  = PA0  = BAT_SENSE   → returns battery_mv ADC count
#   ADC_IN16 = PC0  = SOLAR_SENSE → returns solar_mv ADC count
#   any other channel → returns 0
#
# Magic config registers — peripheral-isolation tests / robot scripts:
#   +0x3F0  write -> set battery_mv     (read also returns battery_mv)
#   +0x3F4  write -> set solar_mv       (read also returns solar_mv)
#   +0x3F8  write -> set chg_stat_n bit (read also returns chg_stat_n)
# Offsets are <0x400 so the model fits in a single STM32 ADC register
# window (default 0x400 stride).

import os

BAT_CHANNEL_MASK   = 1 << 5
SOLAR_CHANNEL_MASK = 1 << 16

# Silence linter warnings about Renode-injected globals.
if "request" not in globals():
    try:
        import sys
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


class BatterySimulatorStm32(object):
    """Same charge/discharge model as the ESP32 stub, with channel-aware
    DR (data register) reads.
    """

    def __init__(self):
        self.debug = os.getenv("BATTERY_ADC_DEBUG") == "1"
        self.reset_state()

    def reset_state(self):
        self.battery_mv = 3700
        self.solar_mv   = 0
        self.chg_stat_n = 1
        self.tick_count = 0
        self.chselr     = BAT_CHANNEL_MASK   # firmware writes CHSELR before each conversion

    def handle_write(self, offset, value):
        if offset == 0x28:                   # CHSELR
            self.chselr = int(value) & 0xFFFFF
        elif offset == 0x3F0:                # Magic battery mV
            self.battery_mv = int(value)
        elif offset == 0x3F4:                # Magic solar mV
            self.solar_mv = int(value)
        elif offset == 0x3F8:                # Magic CHG_STAT_N (charge-stat pin)
            self.chg_stat_n = int(value) & 0x1
        # CR / CFGR / SMPR / IER / SQR writes: accepted, ignored.

    def _adc_count(self, mv):
        # 12-bit ADC; STM32 VREF typically 3.3V but the swm-gagan-wle5
        # voltage divider scales 0..6.6V down to 0..3.3V.  Match the
        # ESP32 stub's encoding (mv * 4095 / 6600).
        return (int(mv) * 4095) // 6600

    def _tick_charge_model(self):
        self.tick_count += 1
        is_day = (self.tick_count // 50) % 2 == 0
        if is_day:
            self.solar_mv = min(5000, self.solar_mv + 100)
            if self.battery_mv < 4200:
                self.battery_mv = min(4200, self.battery_mv + 10)
                self.chg_stat_n = 0
            else:
                self.chg_stat_n = 1
        else:
            self.solar_mv = max(0, self.solar_mv - 150)
            self.chg_stat_n = 1
            self.battery_mv = max(3300, self.battery_mv - 5)

    def handle_read(self, offset):
        if offset == 0x00:                   # ISR — EOC|EOSEQ|ADRDY
            return 0x07
        if offset == 0x40:                   # DR — return the active channel reading
            self._tick_charge_model()
            if self.chselr & SOLAR_CHANNEL_MASK:
                return self._adc_count(self.solar_mv)
            # Default to battery (covers BAT_CHANNEL_MASK and unknown
            # channels — matches "firmware always asks for battery first"
            # ordering).
            return self._adc_count(self.battery_mv)
        if offset == 0x3F0:
            return self.battery_mv
        if offset == 0x3F4:
            return self.solar_mv
        if offset == 0x3F8:
            return self.chg_stat_n
        return 0


if "_battery_sim_stm32" not in globals():
    _battery_sim_stm32 = BatterySimulatorStm32()

if request.IsInit:
    _battery_sim_stm32.reset_state()
elif request.IsWrite:
    _battery_sim_stm32.handle_write(request.Offset, request.Value)
elif request.IsRead:
    request.Value = _battery_sim_stm32.handle_read(request.Offset)
