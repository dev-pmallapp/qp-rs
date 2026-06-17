# -*- coding: utf-8 -*-
#
# SPI-NOR flash storage Renode peripheral model.
#
# Models the register-level semantics of a small SPI-NOR flash part (in the
# spirit of a Winbond W25Q80: 8 Mb, 16-byte JEDEC ID, 4 KB sectors). Sized
# down to 16 KB in this stub because the FOTA chunk-persistence path the
# firmware exercises is page-write / sector-erase oriented — capacity is
# not what the §2.3 coverage targets.
#
# Production firmware on ESP32-C6 uses esp-storage (internal flash) — this
# stub is exclusively for peripheral-isolation tests under
# configs/renode/tests/peripherals/flash/.
#
# Base address (test-only): 0x60020000  Size: 0x8000
#
# Layout (all access is 32-bit DoubleWord):
#   0x0000..0x3FFF   16 KB flash content window (4 sectors × 4 KB).
#                    Default state after erase is 0xFFFFFFFF.
#                    Writes are AND-style (program-to-zero): result &= value.
#                    Out-of-range offsets within 0x4000..0x7FFF map to control.
#
#   0x4000  STATUS         read-only — always 0 (busy bit cleared)
#   0x4004  JEDEC_ID       read-only — 0x00EF4014 (vendor=Winbond, mem=NOR, capacity=8Mb)
#   0x4008  ERASE_SECTOR   write: erases the sector whose index is the written value.
#                           read : returns the last sector index erased (or 0xFFFFFFFF).
#   0x400C  WRITE_COUNT    read-only — running total of bytes programmed since reset
#                           (incremented by 4 on each successful content write).
#   0x4010  SECTOR_SIZE    read-only — 4096
#   0x4014  NUM_SECTORS    read-only — 4
#   0x4018  WRITE_PROTECT  read/write — when non-zero, content writes and sector
#                           erases are no-ops (mirrors WPS).

JEDEC_ID         = 0x00EF4014
SECTOR_SIZE      = 4096
NUM_SECTORS      = 4
TOTAL_BYTES      = SECTOR_SIZE * NUM_SECTORS
ERASED_WORD      = 0xFFFFFFFF
CONTENT_END      = 0x4000
CTRL_STATUS      = 0x4000
CTRL_JEDEC_ID    = 0x4004
CTRL_ERASE       = 0x4008
CTRL_WRITE_COUNT = 0x400C
CTRL_SECTOR_SIZE = 0x4010
CTRL_NUM_SECTORS = 0x4014
CTRL_WP          = 0x4018

# Renode common-helper bootstrap.
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


class FlashSimulator(object):
    def __init__(self):
        self.reset_state()

    def reset_state(self):
        self.content = bytearray(b"\xFF" * TOTAL_BYTES)
        self.write_count = 0
        self.write_protect = 0
        self.last_erased = ERASED_WORD

    def _read_word(self, offset):
        # offset must be inside the content window and 4-aligned.
        b = self.content[offset:offset + 4]
        return b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24)

    def _program_word(self, offset, value):
        # SPI-NOR program is bitwise AND: cannot set bits from 0 → 1 without erase.
        existing = self._read_word(offset)
        programmed = existing & (value & 0xFFFFFFFF)
        self.content[offset]     = programmed & 0xFF
        self.content[offset + 1] = (programmed >> 8) & 0xFF
        self.content[offset + 2] = (programmed >> 16) & 0xFF
        self.content[offset + 3] = (programmed >> 24) & 0xFF
        self.write_count += 4

    def handle_write(self, offset, value):
        if offset < CONTENT_END:
            if (offset & 0x3) != 0:
                return
            if self.write_protect:
                return
            self._program_word(offset, value)
            return
        if offset == CTRL_ERASE:
            if self.write_protect:
                return
            sector = int(value)
            if 0 <= sector < NUM_SECTORS:
                start = sector * SECTOR_SIZE
                for i in range(start, start + SECTOR_SIZE):
                    self.content[i] = 0xFF
                self.last_erased = sector
            return
        if offset == CTRL_WP:
            self.write_protect = 1 if int(value) else 0
            return
        # STATUS / JEDEC_ID / WRITE_COUNT / SECTOR_SIZE / NUM_SECTORS: writes ignored.

    def handle_read(self, offset):
        if offset < CONTENT_END:
            if (offset & 0x3) != 0:
                return 0
            return self._read_word(offset)
        if offset == CTRL_STATUS:
            return 0
        if offset == CTRL_JEDEC_ID:
            return JEDEC_ID
        if offset == CTRL_ERASE:
            return self.last_erased
        if offset == CTRL_WRITE_COUNT:
            return self.write_count
        if offset == CTRL_SECTOR_SIZE:
            return SECTOR_SIZE
        if offset == CTRL_NUM_SECTORS:
            return NUM_SECTORS
        if offset == CTRL_WP:
            return self.write_protect
        return 0


if "_flash_sim" not in globals():
    _flash_sim = FlashSimulator()

if request.IsInit:
    _flash_sim.reset_state()
elif request.IsWrite:
    _flash_sim.handle_write(request.Offset, request.Value)
elif request.IsRead:
    request.Value = _flash_sim.handle_read(request.Offset)
