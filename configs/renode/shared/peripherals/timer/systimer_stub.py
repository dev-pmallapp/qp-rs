# -*- coding: utf-8 -*-
#
# ESP32-C6 SYSTIMER stub for Renode (Python.PythonPeripheral).
#
# SYSTIMER base 0x6000A000, runs at 16 MHz (16 000 000 ticks/s).
#
# esp-hal Instant::now() sequence:
#   1. Write UNIT0_OP (+0x04) with UPDATE bit to trigger a snapshot.
#   2. Read  UNIT0_OP until VALUE_VALID (bit 29) is set.
#   3. Read  UNIT0_VALUE_LO (+0x44) twice and UNIT0_VALUE_HI (+0x40) for
#      a consistent 52-bit counter value.
#
# Strategy: advance `tick` by 1 600 (100 µs @ 16 MHz) on each snapshot-write.
# This is fast enough for the firmware's initialisation delays to exit on the
# first elapsed() check, yet slow enough (100 µs) that the HC-SR04 ultrasonic
# busy-wait loops (5 ms / 30 ms timeouts) accumulate enough iterations to
# observe GPIO transitions before timing out.
# VALUE_LO/HI reads return the stable snapshotted value, so the double-read
# for rollover detection always returns the same pair — no spurious re-reads.
# All other registers return 0.  Writes other than UNIT0_OP are discarded.
#
# Register map (offset from peripheral base):
#   +0x04  UNIT0_OP   bit 31 = UPDATE (w), bit 29 = VALUE_VALID (r)
#   +0x08  UNIT1_OP   (same layout, not used by firmware)
#   +0x40  UNIT0_VALUE_HI  bits[19:0] = counter[51:32]
#   +0x44  UNIT0_VALUE_LO  bits[31:0] = counter[31:0]
#   +0x48  UNIT1_VALUE_HI
#   +0x4C  UNIT1_VALUE_LO
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
    tick = 0

elif request.IsWrite:
    if request.Offset == 0x04 or request.Offset == 0x08:
        tick += 1600     # advance 100 µs of 16 MHz ticks per snapshot request

elif request.IsRead:
    reg = request.Offset
    if reg == 0x04 or reg == 0x08:       # UNITx_OP: snapshot always valid
        request.Value = 0x20000000       # bit 29 = VALUE_VALID
    elif reg == 0x44 or reg == 0x4C:     # UNIT0/1_VALUE_LO
        request.Value = tick & 0xFFFFFFFF
    elif reg == 0x40 or reg == 0x48:     # UNIT0/1_VALUE_HI
        request.Value = (tick >> 32) & 0x000FFFFF
    else:
        request.Value = 0
