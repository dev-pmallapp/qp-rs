# -*- coding: utf-8 -*-
#
# ESP32-C6 TIMG0/TIMG1 stub for Renode (Python.PythonPeripheral).
#
# esp-hal's measure_rtc_clock() on ESP32-C6 register layout (differs from C3):
#
# TIMG0 base 0x60008000, TIMG1 base 0x60009000 (handled via offset % 0x1000):
#   +0x68  RTCCALICFG   bit 31 = RTC_CALI_START, bit 15 = RTC_CALI_RDY (DONE)
#   +0x6C  RTCCALICFG1  bits[31:7] = RTC_CALI_VALUE (calibration result)
#            0x02000000 -> (0x02000000 >> 7) = 262144 XTAL cycles per 1024 slow ticks
#            ~ 40 MHz * 1024 / 262144 ~ 156 kHz  (RC_SLOW_CLK is ~150 kHz)
#   +0x80  RTCCALICFG2  bit 0 = RTC_CALI_TIMEOUT -> always 0 (no timeout)
#
# The polling loop reads 0x68 waiting for bit 15 (DONE), then reads 0x80 to
# detect timeouts.  Memory.MappedMemory persists firmware writes so any write
# that clears DONE causes an infinite spin.  This script ignores all writes and
# always returns fixed read values so the loop exits immediately.

if request.IsRead:
    reg = request.Offset % 0x1000
    if reg == 0x68:
        request.Value = 0x00008000   # RTC_CALI_RDY (DONE) = 1
    elif reg == 0x6C:
        request.Value = 0x02000000   # calibration result in bits[31:7]
    else:
        request.Value = 0x00000000   # RTCCALICFG2 timeout=0, all others=0
