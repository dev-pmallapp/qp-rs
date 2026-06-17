# -*- coding: utf-8 -*-
#
# Solar charger Renode peripheral model.
#
# Models the abstract solar-charging semantics consumed by
# crates/swm-core/src/power.rs: voltage → solar_present flag → ChargeSource enum
# → charging_active flag. The production ESP32-C6 firmware reads the same
# information through SAR2 inside battery_adc_stub.py; this stub gives the
# peripheral-isolation suite (configs/renode/tests/peripherals/solar/) a
# dedicated, deterministic model that does not pull in ADC dynamics.
#
# Base address (test-only): 0x60010000  Size: 0x1000
#
# Registers:
#   +0x000  SOLAR_MV          read-only voltage in millivolts
#   +0x004  CHARGE_SOURCE     read-only enum:
#                                 0 = None
#                                 1 = Battery
#                                 2 = Solar
#                                 3 = BatteryAndSolar
#   +0x008  CHARGING_ACTIVE   read-only 0/1 (mirrors active CC/CV charging)
#   +0x00C  SOLAR_PRESENT     read-only 0/1 (set whenever SOLAR_MV > threshold)
#
#   +0xFF0  WRITE: set solar voltage mV (recomputes derived state on the spot)
#           READ : returns the current solar_mv value
#   +0xFF4  WRITE: set night flag (0=day, non-zero=night → forces solar_present=0)
#           READ : returns the current night flag (0 or 1)
#   +0xFF8  WRITE: set battery_full flag (non-zero → charging_active=0 even with sun)
#           READ : returns the current battery_full flag (0 or 1)
#
# Derived-state rules (recomputed on every Init / write / read of derived
# registers — there is no dynamic time-of-day cycling here, in contrast with
# battery_adc_stub.py; isolation suites need predictability):
#
#   solar_present   = (SOLAR_MV > 3500) and not night
#   charge_source   = match (solar_present, charging_active, battery_full):
#                       solar_present and not battery_full → Solar (2)
#                       solar_present and battery_full      → BatteryAndSolar (3)
#                       else                                → Battery (1) if mv>0
#                                                          → None (0) otherwise
#   charging_active = solar_present and not battery_full

SOLAR_PRESENT_THRESHOLD_MV = 3500

# Renode common-helper bootstrap (matches battery_adc_stub.py).
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


class SolarChargerSimulator(object):
    def __init__(self):
        self.reset_state()

    def reset_state(self):
        self.solar_mv = 0
        self.night = 0
        self.battery_full = 0
        self._recompute()

    def _recompute(self):
        self.solar_present = 1 if (self.solar_mv > SOLAR_PRESENT_THRESHOLD_MV and not self.night) else 0
        self.charging_active = 1 if (self.solar_present and not self.battery_full) else 0
        if self.solar_present and not self.battery_full:
            self.charge_source = 2  # Solar
        elif self.solar_present and self.battery_full:
            self.charge_source = 3  # BatteryAndSolar (float mode)
        elif self.solar_mv > 0 and not self.battery_full:
            self.charge_source = 1  # Battery
        else:
            self.charge_source = 0  # None

    def handle_write(self, offset, value):
        if offset == 0xFF0:
            self.solar_mv = int(value)
        elif offset == 0xFF4:
            self.night = 1 if int(value) else 0
        elif offset == 0xFF8:
            self.battery_full = 1 if int(value) else 0
        else:
            return
        self._recompute()

    def handle_read(self, offset):
        if offset == 0x000:
            return self.solar_mv
        if offset == 0x004:
            return self.charge_source
        if offset == 0x008:
            return self.charging_active
        if offset == 0x00C:
            return self.solar_present
        if offset == 0xFF0:
            return self.solar_mv
        if offset == 0xFF4:
            return self.night
        if offset == 0xFF8:
            return self.battery_full
        return 0


if "_solar_sim" not in globals():
    _solar_sim = SolarChargerSimulator()

if request.IsInit:
    _solar_sim.reset_state()
elif request.IsWrite:
    _solar_sim.handle_write(request.Offset, request.Value)
elif request.IsRead:
    request.Value = _solar_sim.handle_read(request.Offset)
