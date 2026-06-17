# -*- coding: utf-8 -*-
#
# Shared Mock definitions for Renode IDE static analysis linter silencing.
#


class PReg(object):
    """
    Peripheral register model.

    width:    register width in bits — 8, 16, 32, or 64
    reset:    value restored by reset()
    sticky:   mask of bits that write() cannot clear; only clear() can reset them
              (use for hw-set status/IRQ flags that firmware clears explicitly)
    reserved: mask of bits that write() cannot change at all (read-only / reserved)
    """

    def __init__(self, width=8, reset=0, sticky=0, reserved=0):
        self._mask     = (1 << width) - 1
        self._reset    = reset    & self._mask
        self._sticky   = sticky   & self._mask
        self._reserved = reserved & self._mask
        self._val      = self._reset

    @property
    def value(self):
        return self._val

    def reset(self):
        self._val = self._reset

    def write(self, val):
        """Firmware write: sticky and reserved bits are write-ignored."""
        protect   = self._sticky | self._reserved
        self._val = ((val & self._mask) & ~protect) | (self._val & protect)

    def read(self):
        return self._val

    def isSet(self, mask):
        """True if ALL bits in mask are set."""
        return (self._val & mask) == mask

    def isClear(self, mask):
        """True if ALL bits in mask are clear."""
        return (self._val & mask) == 0

    def set(self, mask):
        """Model sets bits — bypasses write protection."""
        self._val = (self._val | mask) & self._mask

    def clear(self, mask):
        """Model clears bits — bypasses write protection, including sticky."""
        self._val = self._val & ~mask & self._mask

    def toggle(self, mask):
        """Model toggles bits."""
        self._val = (self._val ^ mask) & self._mask


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
