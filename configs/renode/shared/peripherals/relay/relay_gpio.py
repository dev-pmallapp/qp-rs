# -*- coding: utf-8 -*-
#
# Relay + valve GPIO peripheral for STM32G0B1 (Dhara / Nalvar roles).
#
# Models GPIOB on the swm-dhara-g0b1 / swm-nalvar-g0b1 boards (SCH_SWM
# §4 / §5):
#
#   PB3 = RELAY_1  (output; pump starter / first relay channel)
#   PB4 = RELAY_2  (output; valve or second relay channel)
#   PB5 = FDBK_1   (input;  opto-isolated feedback for RELAY_1)
#   PB6 = FDBK_2   (input;  opto-isolated feedback for RELAY_2)
#
# STM32G0 GPIO port register block (RM0444 §6.4):
#   +0x10  IDR   — input data register (read; FDBK bits)
#   +0x14  ODR   — output data register (read/write; RELAY bits)
#   +0x18  BSRR  — bits[15:0] set, bits[31:16] reset
#   +0x28  BRR   — bits[15:0] reset (G0+ only)
#   all other offsets: writes ignored, reads return 0.
#
# Observable side-effects for the Robot suite:
#   +0x3E0  RELAY_STATE — read returns (relay1 | (relay2 << 1))
#   +0x3E4  RELAY1_TOGGLES — read returns count of off→on edges on PB3
#   +0x3E8  RELAY2_TOGGLES — read returns count of off→on edges on PB4
#   +0x3F0  FDBK_INJECT  — write sets feedback bits used in IDR responses
#                          (bit 0 → FDBK_1 / PB5, bit 1 → FDBK_2 / PB6)
#                          read returns the current injection mask
#
# All magic offsets are <0x400 so the stub fits inside a single STM32
# GPIO port register window when registered at GPIOB (0x50000400).

RELAY1_BIT = 1 << 3
RELAY2_BIT = 1 << 4
FDBK1_BIT  = 1 << 5
FDBK2_BIT  = 1 << 6

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
    odr             = 0
    fdbk_inject     = 0     # bits 0/1 set by Robot to spoof feedback
    relay1_toggles  = 0
    relay2_toggles  = 0
    relay1_prev     = 0
    relay2_prev     = 0


def _apply_odr_change(prev_odr, new_odr):
    global relay1_toggles, relay2_toggles, relay1_prev, relay2_prev
    r1_new = 1 if (new_odr & RELAY1_BIT) else 0
    r2_new = 1 if (new_odr & RELAY2_BIT) else 0
    if r1_new and not relay1_prev:
        relay1_toggles += 1
    if r2_new and not relay2_prev:
        relay2_toggles += 1
    relay1_prev = r1_new
    relay2_prev = r2_new


if request.IsWrite:
    off = request.Offset

    if off == 0x14:        # ODR direct write
        prev = odr
        odr  = request.Value & 0xFFFF
        _apply_odr_change(prev, odr)

    elif off == 0x18:      # BSRR
        set_bits   = request.Value & 0xFFFF
        reset_bits = (request.Value >> 16) & 0xFFFF
        prev       = odr
        odr        = (odr | set_bits) & 0xFFFF
        odr        = odr & (~reset_bits & 0xFFFF)
        _apply_odr_change(prev, odr)

    elif off == 0x28:      # BRR — bit reset only
        reset_bits = request.Value & 0xFFFF
        prev       = odr
        odr        = odr & (~reset_bits & 0xFFFF)
        _apply_odr_change(prev, odr)

    elif off == 0x3F0:     # Feedback injection register
        fdbk_inject = int(request.Value) & 0x3

    # MODER/OTYPER/OSPEEDR/PUPDR/AFRL/AFRH/LCKR: accepted, ignored.

elif request.IsRead:
    off = request.Offset

    if off == 0x10:        # IDR — feedback bits driven by inject register
        idr_value = 0
        if fdbk_inject & 0x1:
            idr_value |= FDBK1_BIT
        if fdbk_inject & 0x2:
            idr_value |= FDBK2_BIT
        request.Value = idr_value

    elif off == 0x14:      # ODR read-back
        request.Value = odr

    elif off == 0x3E0:     # RELAY_STATE
        r1 = 1 if (odr & RELAY1_BIT) else 0
        r2 = 1 if (odr & RELAY2_BIT) else 0
        request.Value = r1 | (r2 << 1)

    elif off == 0x3E4:     # RELAY1_TOGGLES
        request.Value = relay1_toggles

    elif off == 0x3E8:     # RELAY2_TOGGLES
        request.Value = relay2_toggles

    elif off == 0x3F0:     # FDBK_INJECT read-back
        request.Value = fdbk_inject

    else:
        request.Value = 0
