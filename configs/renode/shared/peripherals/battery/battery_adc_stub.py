# -*- coding: utf-8 -*-
import os

# ESP32-C6 ADC stub with dynamic battery/solar charge & discharge simulation.

# Base: 0x6000E000  Size: 0x1000

# Real-register reads return values that satisfy the esp-hal one-shot ADC driver:
#   +0x40  SAR1DATA_STATUS - bits[16:0]: bit 16=VALID, bits[11:0]=BAT_SENSE ADC count
#   +0x44  SAR2DATA_STATUS - bits[16:0]: bit 16=VALID, bits[11:0]=SOLAR_SENSE ADC count

# Magic config registers let the Renode monitor script override voltages:
#   +0xFF0  write -> set battery_mv
#   +0xFF4  write -> set solar_mv
#   +0xFF8  write -> set chg_stat_n (bit 0: 0=charging, 1=not charging)

# ------------------------------------------------------------
# Import Renode common helpers – executed only once
if "request" not in globals():
    try:
        import sys, os
        sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        from renode_swm.common import *  # noqa: E402
    except ImportError:
        # Minimal mocks for offline testing / IDE linting
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

        class MockAntmicro(object):
            class Renode(object):
                class Logging(object):
                    class LogLevel(object):
                        Info = 0
                        Warning = 1
                        Error = 2
        Antmicro = MockAntmicro()

# ------------------------------------------------------------
# Battery simulator – encapsulates all mutable state
class BatterySimulator:
    def __init__(self):
        self.debug = os.getenv("BATTERY_ADC_DEBUG") == "1"
        self.reset_state()

    def _log(self, msg):
        if self.debug:
            # Use Renode's logger if available
            try:
                self._logger(Antmicro.Renode.Logging.LogLevel.Info, msg)
            except Exception:
                pass

    def _logger(self, level, msg):
        # Renode scripts expose a "self" object with a Log method
        if "self" in globals() and hasattr(self, "Log"):
            self.Log(level, msg)
        else:
            # Fallback for mock environment – ignore
            pass

    def reset_state(self):
        self.battery_mv = 3700   # default battery voltage (mV)
        self.solar_mv = 0        # default solar voltage (mV)
        self.chg_stat_n = 1      # idle (not charging)
        self.tick_count = 0
        self._log("[BatterySimulator] State reset")

    def handle_write(self, offset, value):
        if offset == 0xFF0:
            self.battery_mv = int(value)
        elif offset == 0xFF4:
            self.solar_mv = int(value)
        elif offset == 0xFF8:
            self.chg_stat_n = int(value) & 0x1
        self._log("[BatterySimulator] Write offset 0x{:X} value {}".format(offset, value))

    def handle_read(self, offset):
        # Update dynamic behaviour on each sense read
        if offset in (0x40, 0x44):
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
            self._log("[BatterySimulator] Tick {}, day={}".format(self.tick_count, is_day))

        # Return values for requested registers
        if offset == 0xFF0:
            return self.battery_mv
        elif offset == 0xFF4:
            return self.solar_mv
        elif offset == 0xFF8:
            return self.chg_stat_n
        elif offset == 0x40:
            adc = (self.battery_mv * 4095) // 6600
            return (1 << 16) | (adc & 0xFFF)
        elif offset == 0x44:
            adc = (self.solar_mv * 4095) // 6600
            return (1 << 16) | (adc & 0xFFF)
        else:
            return 0

# ------------------------------------------------------------
# Global simulator instance – preserved across Renode reloads.
# State is only reset via request.IsInit (machine reload) — never on every access.
if "_battery_sim" not in globals():
    _battery_sim = BatterySimulator()

# ------------------------------------------------------------
# Renode request handling
if request.IsInit:
    _battery_sim.reset_state()
elif request.IsWrite:
    _battery_sim.handle_write(request.Offset, request.Value)
elif request.IsRead:
    request.Value = _battery_sim.handle_read(request.Offset)
