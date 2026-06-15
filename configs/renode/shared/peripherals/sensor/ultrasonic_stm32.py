# -*- coding: utf-8 -*-
#
# HC-SR04 ultrasonic sensor simulation via STM32 GPIO registers.
#
# STM32 variant of `ultrasonic_hcsr04.py` (which decodes ESP32-C6 GPIO
# register layout).  Same trigger/echo protocol — only the bus-side
# register decode changes.
#
# Pinout (from stm32wle5_devkit.repl):
#   PA8  = USON_PWR_EN (output, ignored)
#   PA9  = USON_TRIG   (output from firmware)
#   PA10 = USON_ECHO   (input  to   firmware)
#
# STM32 GPIO port register block (RM0461 §11 / RM0444 §6, 0x400-byte stride):
#   +0x00  MODER   — pin mode (per-pin 2-bit field); writes accepted, ignored
#   +0x04  OTYPER  — accepted, ignored
#   +0x08  OSPEEDR — accepted, ignored
#   +0x0C  PUPDR   — accepted, ignored
#   +0x10  IDR     — input data register (read; echo bit reported here)
#   +0x14  ODR     — output data register (read/write; trigger bit tracked)
#   +0x18  BSRR    — bit set/reset (write; bits[15:0] set, bits[31:16] reset)
#   +0x28  BRR     — bit reset (write; G0+ only; bits[15:0] reset)
#   all other offsets: writes ignored, reads return 0.
#
# Magic config register (peripheral-isolation tests only — like the ESP32
# stub, production firmware never accesses it):
#   +0x3F0  write -> set distance_mm (refreshes pulse_reads)
#           read  -> returns current distance_mm
#
# Sized at 0x400 to occupy exactly one GPIO port block; placed in the
# repl in place of `gpioPortA` (the .repl Unregisters gpioPortA and adds
# this peripheral at the same base 0x48000000 — same model the ESP32
# port uses for gpio_hcsr04).

TRIGGER_BIT = 1 << 9
ECHO_BIT    = 1 << 10

DEFAULT_DISTANCE_MM = 800


def _pulse_reads_for(distance_mm):
    # Each wait_until iteration consumes one systimer snapshot (100 µs).
    return max(2, (int(distance_mm) * 2000) // (343 * 100))


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


if request.IsInit:
    odr          = 0
    trigger_high = False
    # echo_state: 'idle' | 'pre' | 'high'
    echo_state   = 'idle'
    echo_counter = 0
    distance_mm  = DEFAULT_DISTANCE_MM
    pulse_reads  = _pulse_reads_for(distance_mm)

elif request.IsWrite:
    off = request.Offset

    if off == 0x14:        # ODR direct write
        prev = odr
        odr  = request.Value & 0xFFFF
        if (odr & TRIGGER_BIT) and not (prev & TRIGGER_BIT):
            trigger_high = True
        elif trigger_high and not (odr & TRIGGER_BIT):
            trigger_high = False
            echo_state   = 'pre'
            echo_counter = 1

    elif off == 0x18:      # BSRR
        set_bits   = request.Value & 0xFFFF
        reset_bits = (request.Value >> 16) & 0xFFFF
        prev       = odr
        odr        = (odr | set_bits) & 0xFFFF
        odr        = odr & (~reset_bits & 0xFFFF)
        # Trigger rising edge: BSRR set-bit asserted with TRIGGER_BIT.
        if (set_bits & TRIGGER_BIT) and not (prev & TRIGGER_BIT):
            trigger_high = True
        # Trigger falling edge: BSRR reset-bit clears TRIGGER_BIT while
        # the model believes it is high — start the echo cycle.
        if trigger_high and (reset_bits & TRIGGER_BIT):
            trigger_high = False
            echo_state   = 'pre'
            echo_counter = 1

    elif off == 0x28:      # BRR — bit reset only (STM32G0/L4+)
        reset_bits = request.Value & 0xFFFF
        if trigger_high and (reset_bits & TRIGGER_BIT):
            trigger_high = False
            echo_state   = 'pre'
            echo_counter = 1
        odr = odr & (~reset_bits & 0xFFFF)

    elif off == 0x3F0:     # Magic distance-injection register
        distance_mm = int(request.Value)
        pulse_reads = _pulse_reads_for(distance_mm)

    # MODER/OTYPER/OSPEEDR/PUPDR/AFRL/AFRH/LCKR: accepted, ignored.

elif request.IsRead:
    off = request.Offset

    if off == 0x10:        # IDR — input data register
        if echo_state == 'pre':
            echo_counter -= 1
            if echo_counter <= 0:
                echo_state   = 'high'
                echo_counter = pulse_reads
            request.Value = 0

        elif echo_state == 'high':
            echo_counter -= 1
            if echo_counter <= 0:
                echo_state    = 'idle'
                request.Value = 0
            else:
                request.Value = ECHO_BIT

        else:
            request.Value = 0

    elif off == 0x14:      # ODR read-back
        request.Value = odr

    elif off == 0x3F0:     # Magic distance read-back
        request.Value = distance_mm

    else:
        request.Value = 0
