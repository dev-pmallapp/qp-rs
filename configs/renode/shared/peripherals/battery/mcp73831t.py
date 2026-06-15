# -*- coding: utf-8 -*-
#
# MCP73831T Li-Ion/Li-Polymer Charge Management IC — Renode peripheral model.
#
# Replaces battery_adc_stub.py for the Gagan node.  Implements the full
# MCP73831T charge state machine on top of the same ESP32-C6 ADC register
# interface so existing Robot tests that drive the magic registers continue
# to work without modification.
#
# Hardware context (Gagan RevA, ports/esp32c6/src/board/battery.rs):
#   GPIO0 / ADC1_CH0 → BAT_SENSE  (battery voltage divider, 2:1, 0..6.6V range)
#   GPIO1 / ADC1_CH1 → SOLAR_SENSE (solar panel divider,  2:1, 0..6.6V range)
#   GPIO3             → CHG_STAT_N  (open-drain output of MCP73831T STAT pin;
#                                    pulled HIGH by firmware Input::Pull::Up)
#
# ADC register interface (base 0x6000E000, same as battery_adc_stub.py):
#   +0x40  SAR1DATA_STATUS — bit 16=VALID, bits[11:0]=BAT_SENSE ADC count
#   +0x44  SAR2DATA_STATUS — bit 16=VALID, bits[11:0]=SOLAR_SENSE ADC count
#
# Magic registers (test injection / Robot scripts):
#   +0xFF0  r/w → battery_mv   (mV; also writable to force a voltage)
#   +0xFF4  r/w → solar_mv     (mV)
#   +0xFF8  r/w → chg_stat_n   (0 = charging/STAT-low, 1 = idle/STAT-hi-z)
#                               Writing this overrides the state machine output
#                               until the next ADC read tick recomputes it.
#   +0xFFC  r/w → season       (outdoor scenario control)
#                   0 = AUTO     — default alternating day/night (50-tick phases)
#                   1 = SUNNY    — always daytime; solar charges at full rate
#                   2 = CLOUDY   — solar absent; MCP stays SHUTDOWN; battery drains
#                   3 = CRITICAL — like CLOUDY but forces battery_mv to 3100 mV
#                                  on write (immediately below the BatteryLow
#                                  firmware threshold of ~3400 mV / 20% SOC)
#
# MCP73831T state machine:
#   SHUTDOWN   — VIN < VIN_MIN (3.75V): no charging, STAT = Hi-Z
#   PRECHARGE  — VIN ≥ VIN_MIN AND VBAT < VLOW (3.0V): 10% IFAST, STAT = LOW
#   FAST_CHARGE— VIN ≥ VIN_MIN AND VLOW ≤ VBAT < VREG (4.2V): IFAST, STAT = LOW
#   COMPLETE   — VBAT ≥ VREG: charge done, STAT = Hi-Z
#                Re-arms to FAST_CHARGE when VBAT < VREG − VHYS (4.0V)
#
# NOTE: GPIO3 (CHG_STAT_N) is read by the ESP32-C6 GPIO peripheral at
# 0x60091000 (ultrasonic_hcsr04.py).  That model exposes a matching magic
# register at +0xFF8 so a Robot test can sync both sides:
#   sysbus WriteDoubleWord 0x6000EFF8 0   # mcp73831t charging
#   sysbus WriteDoubleWord 0x60091FF8 0   # gpio_hcsr04 mirrors same state

import os

# ── MCP73831T electrical constants ──────────────────────────────────────────
VREG     = 4200   # mV  charge termination voltage (regulation voltage)
VLOW     = 3000   # mV  precharge → fast-charge threshold
VIN_MIN  = 3750   # mV  minimum VIN to enable charging  (≈ VBAT_max + diode drop)
VHYS     = 200    # mV  re-charge hysteresis (re-arm threshold = VREG − VHYS)

# State machine identifiers
_SHUTDOWN    = 0
_PRECHARGE   = 1
_FAST_CHARGE = 2
_COMPLETE    = 3

# Season identifiers (magic register +0xFFC)
_SEASON_AUTO     = 0   # alternating day/night every TICKS_PER_PHASE
_SEASON_SUNNY    = 1   # always daytime: solar charges at full rate
_SEASON_CLOUDY   = 2   # overcast: solar absent, MCP stays SHUTDOWN, battery drains
_SEASON_CRITICAL = 3   # like CLOUDY but forces battery_mv to 3100 mV on write

# Silence IDE linter warnings about Renode-injected globals.
if "request" not in globals():
    try:
        import sys
        sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        from renode_swm.common import *  # noqa: E402
    except ImportError:
        class MockRequest(object):
            IsInit = False; IsWrite = False; IsRead = False; Offset = 0; Value = 0
        request = MockRequest()
        class MockSelf(object):
            def Log(self, level, msg): pass
        self = MockSelf()
        class MockAntmicro(object):
            class Renode(object):
                class Logging(object):
                    class LogLevel(object):
                        Info = 0; Warning = 1; Error = 2
        Antmicro = MockAntmicro()


class Mcp73831tSim(object):
    """MCP73831T Li-Ion charge management IC simulation."""

    # Charge rate per ADC tick (simplified time model):
    #   50 ticks ≈ one simulated half-day (day or night phase).
    #   Day:   solar_mv rises, battery charges at ~10 mV/tick.
    #   Night: solar_mv falls, battery drains at ~5 mV/tick.
    TICKS_PER_PHASE = 50

    def __init__(self):
        self.debug = os.getenv("MCP73831T_DEBUG") == "1"
        self.reset_state()

    def reset_state(self):
        self.battery_mv  = 3700
        self.solar_mv    = 0
        self.tick_count  = 0
        self.chg_state   = _SHUTDOWN
        self.chg_stat_n  = 1   # 1 = not charging
        self.season      = _SEASON_AUTO
        self._stat_override = None  # set by magic-reg write; cleared on next tick

    # ── internal helpers ────────────────────────────────────────────────────

    def _adc_count(self, mv):
        """12-bit ADC count for a millivolt value (2:1 divider, 3.3V ref)."""
        return (int(mv) * 4095) // 6600

    def _compute_state(self):
        vin = self.solar_mv  # VIN = solar panel output (proxy)
        vbat = self.battery_mv
        if vin < VIN_MIN:
            return _SHUTDOWN
        if vbat >= VREG:
            return _COMPLETE
        if vbat < VLOW:
            return _PRECHARGE
        return _FAST_CHARGE

    def _tick(self):
        """Advance the simulation by one ADC-read step."""
        self.tick_count += 1

        if self.season in (_SEASON_CLOUDY, _SEASON_CRITICAL):
            # No solar input — MCP73831T stays SHUTDOWN; battery drains slowly.
            # Floor at 3000 mV keeps voltage in a valid ADC range while still
            # well below the firmware BatteryLow threshold (~3400 mV / 20% SOC).
            self.solar_mv = 0
            self.battery_mv = max(3000, self.battery_mv - 5)
            self.chg_state = _SHUTDOWN
        else:
            # Determine solar for this tick
            if self.season == _SEASON_SUNNY:
                self.solar_mv = min(5000, self.solar_mv + 100)
                night_drain = False
            else:  # _SEASON_AUTO
                is_day = (self.tick_count // self.TICKS_PER_PHASE) % 2 == 0
                if is_day:
                    self.solar_mv = min(5000, self.solar_mv + 100)
                else:
                    self.solar_mv = max(0, self.solar_mv - 150)
                night_drain = not is_day

            # Re-evaluate MCP73831T charge state machine
            prev_state = self.chg_state
            new_state  = self._compute_state()

            # COMPLETE re-arms to FAST_CHARGE only after hysteresis drop
            if prev_state == _COMPLETE and self.battery_mv >= (VREG - VHYS):
                new_state = _COMPLETE

            self.chg_state = new_state

            # Apply charge current or quiescent drain
            if self.chg_state == _PRECHARGE:
                self.battery_mv = min(VLOW, self.battery_mv + 2)   # 10% IFAST
            elif self.chg_state == _FAST_CHARGE:
                self.battery_mv = min(VREG, self.battery_mv + 10)  # IFAST
            elif night_drain:
                # AUTO night: quiescent discharge (no solar, no active charging)
                self.battery_mv = max(3300, self.battery_mv - 5)

        # Update STAT output (override clears on each tick unless re-written)
        if self._stat_override is not None:
            self.chg_stat_n = self._stat_override
            self._stat_override = None
        else:
            self.chg_stat_n = 0 if self.chg_state in (_PRECHARGE, _FAST_CHARGE) else 1

        if self.debug:
            season_name = ["AUTO", "SUNNY", "CLOUDY", "CRITICAL"][self.season]
            state_name  = ["SHUTDOWN", "PRECHARGE", "FAST_CHG", "COMPLETE"][self.chg_state]
            try:
                self.Log(Antmicro.Renode.Logging.LogLevel.Info,
                         "[MCP73831T] tick={} season={} state={} bat={}mV sol={}mV STAT={}".format(
                             self.tick_count, season_name, state_name,
                             self.battery_mv, self.solar_mv, self.chg_stat_n))
            except Exception:
                pass

    # ── Renode request handlers ─────────────────────────────────────────────

    def handle_write(self, offset, value):
        if offset == 0xFF0:
            self.battery_mv = int(value)
        elif offset == 0xFF4:
            self.solar_mv = int(value)
        elif offset == 0xFF8:
            # Magic override: also pins the state until next tick
            self.chg_stat_n = int(value) & 0x1
            self._stat_override = self.chg_stat_n
        elif offset == 0xFFC:
            self.season = int(value) & 0x3
            if self.season == _SEASON_CRITICAL:
                # Immediately force battery to a level below the firmware
                # BatteryLow threshold (~3400 mV / 20% SOC Li-Ion).
                self.battery_mv = 3100
                self.solar_mv   = 0
                self.chg_stat_n = 1
                self.chg_state  = _SHUTDOWN

    def handle_read(self, offset):
        if offset in (0x40, 0x44):
            self._tick()
        if offset == 0x40:   # SAR1DATA_STATUS — battery
            return (1 << 16) | (self._adc_count(self.battery_mv) & 0xFFF)
        if offset == 0x44:   # SAR2DATA_STATUS — solar
            return (1 << 16) | (self._adc_count(self.solar_mv) & 0xFFF)
        if offset == 0xFF0:
            return self.battery_mv
        if offset == 0xFF4:
            return self.solar_mv
        if offset == 0xFF8:
            return self.chg_stat_n
        if offset == 0xFFC:
            return self.season
        return 0


# ── Global singleton — persists across Renode reloads ───────────────────────
if "_mcp73831t_sim" not in globals():
    _mcp73831t_sim = Mcp73831tSim()

# ── Renode dispatch ─────────────────────────────────────────────────────────
if request.IsInit:
    _mcp73831t_sim.reset_state()
elif request.IsWrite:
    _mcp73831t_sim.handle_write(request.Offset, request.Value)
elif request.IsRead:
    request.Value = _mcp73831t_sim.handle_read(request.Offset)
