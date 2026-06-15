# -*- coding: utf-8 -*-
#
# ESP32-C6 SYSTIMER stub for Renode — MC (Pramukh / coordinator) variant.
#
# Identical protocol to systimer_stub.py but advances 16 000 ticks per
# snapshot request instead of 1 600.  At 16 MHz that is 1 ms per
# Instant::now() call.
#
# Rationale: the sensor-node (Gagan) firmware accumulates simulated time
# naturally through thousands of HC-SR04 busy-wait loop iterations.  The MC
# (Pramukh) has no sensor busy-loop, so Instant::now() is called only once
# per QS event — giving ~100 µs of virtual time per event with the default
# 1 600-tick increment, which qspy rounds to 0 ms.  With 16 000 ticks each
# QS event advances the display timestamp by ~1 ms, making trace output
# readable in simulation.  Real hardware uses the live hardware counter and
# is unaffected by this stub.
#
# Register map (identical to systimer_stub.py):
#   +0x04  UNIT0_OP   bit 31 = UPDATE (w), bit 29 = VALUE_VALID (r)
#   +0x40  UNIT0_VALUE_HI  bits[19:0] = counter[51:32]
#   +0x44  UNIT0_VALUE_LO  bits[31:0] = counter[31:0]
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
    tick = 0

elif request.IsWrite:
    if request.Offset == 0x04 or request.Offset == 0x08:
        tick += 16000    # 1 ms at 16 MHz per snapshot request

elif request.IsRead:
    reg = request.Offset
    if reg == 0x04 or reg == 0x08:
        request.Value = 0x20000000       # bit 29 = VALUE_VALID
    elif reg == 0x44 or reg == 0x4C:
        request.Value = tick & 0xFFFFFFFF
    elif reg == 0x40 or reg == 0x48:
        request.Value = (tick >> 32) & 0x000FFFFF
    else:
        request.Value = 0
