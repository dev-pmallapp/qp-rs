# -*- coding: utf-8 -*-
#
# HC-SR04 ultrasonic sensor simulation via ESP32-C6 GPIO registers.
#
# Replaces the gpio_stub MappedMemory so Renode can drive the HC-SR04
# trigger/echo protocol used by EspUltrasonicLevelSensor in swm-rs.
#
# Pinout (from ports/esp32c6/src/board.rs):
#   GPIO5 = power_enable  (output, ignored in simulation)
#   GPIO6 = trigger       (output from firmware)
#   GPIO7 = echo          (input  to   firmware)
#
# ESP32-C6 GPIO registers handled (base 0x60091000).
# Offsets from the esp32c6 PAC RegisterBlock (esp32c6-0.23.2/src/gpio.rs):
#   +0x004  out          GPIO output register for GPIO0-31      (direct write)
#   +0x008  out_w1ts     GPIO output SET  register for GPIO0-31 (atomic)
#   +0x00C  out_w1tc     GPIO output CLEAR register for GPIO0-31(atomic)
#   +0x020  enable       GPIO output enable for GPIO0-31        (ignored)
#   +0x024  enable_w1ts  GPIO output enable SET for GPIO0-31    (ignored)
#   +0x028  enable_w1tc  GPIO output enable CLEAR for GPIO0-31  (ignored)
#   +0x03C  in_          GPIO INPUT register for GPIO0-31       ← NOT 0x030!
#   all other offsets: writes ignored, reads return 0
#
# Protocol:
#   Firmware calls pulse_trigger() → GPIO6 LOW→HIGH (10 µs) →LOW.
#   On GPIO6 HIGH→LOW: peripheral enters 'pre' state (echo still LOW).
#   'pre' state lasts exactly 1 GPIO_IN read so wait_while(HIGH,5ms)
#   sees echo LOW and exits immediately.
#   Then echo goes HIGH for PULSE_READS reads.
#   wait_until(HIGH, 5ms) sees HIGH on first check and exits.
#   wait_until(LOW,  30ms) counts PULSE_READS-1 reads then sees LOW.
#   pulse_us ≈ (PULSE_READS + 1) × 100 µs  (100 µs / systimer snapshot).
#
# Calibration:
#   DISTANCE_MM   → set the simulated water-to-sensor distance.
#   pulse_us      = DISTANCE_MM × 2000 / 343   (µs, speed of sound)
#   PULSE_READS   ≈ pulse_us / 100             (100 µs per snapshot)
#
# IMPORTANT: systimer_stub.py must be set to 1600 ticks/snapshot (100 µs)
# for the 5 ms and 30 ms timeout loops to have enough iterations.
#
# Magic config register (peripheral-isolation tests only; firmware never
# accesses it because the production firmware short-circuits read_sample
# to RENODE_INJECTED_DISTANCE_MM under cfg(feature = "renode")):
#   +0xFF0  write -> set distance_mm (rounds-down, refreshes pulse_reads)
#           read  -> returns the current distance_mm value

TRIGGER_BIT    = 1 << 6
ECHO_BIT       = 1 << 7
# GPIO3 = CHG_STAT_N (active-low open-drain from MCP73831T STAT pin).
# Pull-up on the firmware side (Input::Pull::Up) makes the pin HIGH when not
# charging.  Default here is 1 (not charging) so firmware starts with
# ChargeSource::None.  Robot tests / monitor scripts can write magic reg
# 0xFF8 to override — and should write the same value to the MCP73831T magic
# reg at 0x6000EFF8 to keep both models consistent.
CHG_STAT_N_BIT = 1 << 3

# Default simulated distance (mm).
DEFAULT_DISTANCE_MM = 800

def _pulse_reads_for(distance_mm):
    # Each wait_until(false) iteration consumes one systimer snapshot (100 µs).
    return max(2, (int(distance_mm) * 2000) // (343 * 100))
# To silence IDE linter warnings about injected Renode globals:
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

        class MockAntmicro(object):
            class Renode(object):
                class Logging(object):
                    class LogLevel(object):
                        Info = 0
                        Warning = 1
                        Error = 2
        Antmicro = MockAntmicro()

if request.IsInit:
    gpio_out     = 0
    trigger_high = False
    # echo_state: 'idle' | 'pre' | 'high'
    echo_state   = 'idle'
    echo_counter = 0
    distance_mm  = DEFAULT_DISTANCE_MM
    pulse_reads  = _pulse_reads_for(distance_mm)
    chg_stat_n   = 1   # 1 = STAT Hi-Z (not charging), 0 = STAT LOW (charging)

elif request.IsWrite:
    off = request.Offset

    if off == 0x008:    # GPIO_OUT_W1TS
        prev = gpio_out
        gpio_out |= request.Value & 0x3FFFFFFF
        if (gpio_out & TRIGGER_BIT) and not (prev & TRIGGER_BIT):
            trigger_high = True

    elif off == 0x00C:  # GPIO_OUT_W1TC
        if trigger_high and (request.Value & TRIGGER_BIT):
            # Trigger pulse complete — begin echo cycle.
            trigger_high = False
            echo_state   = 'pre'
            echo_counter = 1    # one LOW read consumed by wait_while
        gpio_out &= ~(request.Value & 0x3FFFFFFF)

    elif off == 0x004:  # GPIO_OUT direct
        prev     = gpio_out
        gpio_out = request.Value & 0x3FFFFFFF
        if (gpio_out & TRIGGER_BIT) and not (prev & TRIGGER_BIT):
            trigger_high = True
        elif trigger_high and not (gpio_out & TRIGGER_BIT):
            trigger_high = False
            echo_state   = 'pre'
            echo_counter = 1

    elif off == 0xFF0:  # Magic distance-injection register (peripheral-isolation tests).
        distance_mm = int(request.Value)
        pulse_reads = _pulse_reads_for(distance_mm)

    elif off == 0xFF8:  # Magic CHG_STAT_N inject (mirrors mcp73831t.py magic 0xFF8).
        chg_stat_n = int(request.Value) & 0x1

    # GPIO_ENABLE (0x020/0x024/0x028) and all other writes are silently ignored.

elif request.IsRead:
    off = request.Offset

    if off == 0x03C:    # in_ — GPIO input register for GPIO0-31 (PAC offset 0x3C)
        # Bit 3 = CHG_STAT_N: HIGH (1) when not charging, LOW (0) when charging.
        chg_bit = CHG_STAT_N_BIT if chg_stat_n else 0

        if echo_state == 'pre':
            echo_counter -= 1
            if echo_counter <= 0:
                # Transition: echo rises HIGH for the pulse.
                echo_state   = 'high'
                echo_counter = pulse_reads
            request.Value = chg_bit     # echo LOW during pre phase

        elif echo_state == 'high':
            echo_counter -= 1
            if echo_counter <= 0:
                # Echo pulse complete — return to idle.
                echo_state    = 'idle'
                request.Value = chg_bit   # echo fell LOW
            else:
                request.Value = ECHO_BIT | chg_bit   # echo still HIGH

        else:
            request.Value = chg_bit

    elif off == 0xFF0:  # Magic distance read-back.
        request.Value = distance_mm

    elif off == 0xFF8:  # Magic CHG_STAT_N read-back.
        request.Value = chg_stat_n

    else:
        request.Value = 0
