*** Settings ***
Documentation     SX1278Radio peripheral model unit tests.
...               Tests drive SPI registers directly — no firmware required.
...               Each test case exercises one protocol path of SX1278Radio.cs
...               and verifies the state machine and wireless-medium delivery.
...
...               SX1278 SPI protocol (one GPSPI2 CMD_USR trigger per operation):
...                 Write reg:  W0 = (val << 8) | (reg | 0x80), then CMD_TRIGGER
...                 Read  reg:  W0 = (reg & 0x7F),              then CMD_TRIGGER
...                             Response is at W0[15:8] after CMD.
...               Key registers:
...                 0x00  RegFifo          — TX accumulate (w) / RX pop (r)
...                 0x01  RegOpMode        — 0x83 = LoRa TX, 0x81 = Standby
...                 0x12  RegIrqFlags      — TxDone=0x08 | RxDone=0x40; w1c
...                 0x22  RegPayloadLength — RX frame byte count (set on ReceiveFrame)

Suite Setup       Prepare Emulation
Suite Teardown    Reset Emulation
Test Teardown     Reset Emulation

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC}            ${CURDIR}${/}test_sx1278_platform.resc

# GPSPI2 register addresses (spi2 base = 0x60081000)
${CMD_REG}         0x60081000
${W0_REG}          0x60081098

# Single value that triggers a GPSPI transaction (USR bit = bit 24)
${CMD_TRIGGER}     0x01000000

# Model-only magic register addresses
${MAGIC_TXCOUNT}   0x60081FF0
${MAGIC_RXSTAGE}   0x60081FF4
${MAGIC_FORCEFL}   0x60081FF8
${MAGIC_RXCOMMIT}  0x60081FFC

# SX1278 RegIrqFlags bit values (as they appear at W0[15:8] after a read)
${TXDONE_BYTE}     0x08
${RXDONE_BYTE}     0x40

*** Test Cases ***

# ── TC-1 ─────────────────────────────────────────────────────────
TX Path Sets TxDone IRQ
    [Documentation]    Write 4 bytes to RegFifo then trigger TX (RegOpMode = 0x83).
    ...                RegIrqFlags must have TxDone (bit 3 = 0x08) set.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SX1278 Frame    0x01    0x02    0x03    0x04

    ${irq}=    Read SX1278 IrqFlags
    Should Be Equal As Integers    ${irq}    ${TXDONE_BYTE}
    ...    msg=TxDone (0x08) must be set after TX

# ── TC-2 ─────────────────────────────────────────────────────────
Gagan TX Delivers RxDone To Pramukh
    [Documentation]    When Gagan triggers TX, the wireless medium calls
    ...                Pramukh's ReceiveFrame synchronously.
    ...                Pramukh's RegIrqFlags must have RxDone (bit 6 = 0x40).
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SX1278 Frame    0x01    0x02    0x03    0x04

    Switch To Machine    SWM-Pramukh-MC
    ${irq}=    Read SX1278 IrqFlags
    Should Be Equal As Integers    ${irq}    ${RXDONE_BYTE}
    ...    msg=RxDone (0x40) must be set on Pramukh after Gagan TX

# ── TC-3 ─────────────────────────────────────────────────────────
Pramukh Receives Exact Payload From Gagan
    [Documentation]    RegPayloadLength (0x22) must equal the transmitted byte
    ...                count and each RegFifo (0x00) pop must return the exact
    ...                bytes Gagan sent, in order.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SX1278 Frame    0x01    0x02    0x03    0x04

    Switch To Machine    SWM-Pramukh-MC
    ${len}=    Read SX1278 Reg    0x22
    Should Be Equal As Integers    ${len}    4    msg=PayloadLength must be 4

    ${b1}=    Read SX1278 Reg    0x00
    ${b2}=    Read SX1278 Reg    0x00
    ${b3}=    Read SX1278 Reg    0x00
    ${b4}=    Read SX1278 Reg    0x00
    Should Be Equal As Integers    ${b1}    0x01    msg=byte 0 mismatch
    Should Be Equal As Integers    ${b2}    0x02    msg=byte 1 mismatch
    Should Be Equal As Integers    ${b3}    0x03    msg=byte 2 mismatch
    Should Be Equal As Integers    ${b4}    0x04    msg=byte 3 mismatch

# ── TC-4 ─────────────────────────────────────────────────────────
IrqFlags Write-1-To-Clear
    [Documentation]    Writing 0x08 to RegIrqFlags clears TxDone.
    ...                Subsequent read must return 0.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SX1278 Frame    0x01    0x02    0x03    0x04

    # Confirm TxDone is set before clearing
    ${irq_pre}=    Read SX1278 IrqFlags
    Should Be Equal As Integers    ${irq_pre}    ${TXDONE_BYTE}
    ...    msg=TxDone must be set before the clear

    # write_reg(0x12, 0x08) — W0 = (0x08 << 8) | (0x12 | 0x80) = 0x00000892
    Write SX1278 Reg    0x12    0x08

    ${irq_post}=    Read SX1278 IrqFlags
    Should Be Equal As Integers    ${irq_post}    0
    ...    msg=IrqFlags must be 0 after write-1-to-clear

# ── TC-5 ─────────────────────────────────────────────────────────
Out Of Range Node Does Not Receive
    [Documentation]    SWM-OOR-Node is at (300,0,0) — 300 units from Gagan.
    ...                Medium range is 200 units so ReceiveFrame must NOT be
    ...                called: OOR IrqFlags must remain 0.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SX1278 Frame    0x01    0x02    0x03    0x04

    Switch To Machine    SWM-OOR-Node
    ${irq}=    Read SX1278 IrqFlags
    Should Be Equal As Integers    ${irq}    0
    ...    msg=Out-of-range node must not receive the frame

# ── TC-6 ─────────────────────────────────────────────────────────
TxCount Magic Register Increments Per Transmission
    [Documentation]    Magic register +0xFF0 (read) returns the number of TX
    ...                operations dispatched, including forced-fail ones.
    ...                Transmit 3 frames — txCount must be 3.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SX1278 Frame    0x01    0x02    0x03    0x04
    Transmit SX1278 Frame    0x05    0x06    0x07    0x08
    Transmit SX1278 Frame    0x09    0x0A    0x0B    0x0C

    ${raw}=    Execute Command    sysbus ReadDoubleWord ${MAGIC_TXCOUNT}
    Should Be Equal As Integers    ${raw.strip()}    3
    ...    msg=txCount magic register must be 3 after 3 transmissions

# ── TC-7 ─────────────────────────────────────────────────────────
Magic Register RX Injection
    [Documentation]    Stage bytes into rxStaging via +0xFF4, commit via +0xFFC.
    ...                RxDone must be set, PayloadLength must equal the staged
    ...                count, and each FIFO pop must return the injected bytes.
    Setup Platform

    Switch To Machine    SWM-Pramukh-MC
    # Stage three bytes: 0x11, 0x22, 0x33
    Execute Command    sysbus WriteDoubleWord ${MAGIC_RXSTAGE}   0x11
    Execute Command    sysbus WriteDoubleWord ${MAGIC_RXSTAGE}   0x22
    Execute Command    sysbus WriteDoubleWord ${MAGIC_RXSTAGE}   0x33
    # Commit staged frame → promotes rxBuffer, sets RxDone
    Execute Command    sysbus WriteDoubleWord ${MAGIC_RXCOMMIT}  0x01

    ${irq}=    Read SX1278 IrqFlags
    Should Be Equal As Integers    ${irq}    ${RXDONE_BYTE}
    ...    msg=RxDone must be set after magic injection

    ${len}=    Read SX1278 Reg    0x22
    Should Be Equal As Integers    ${len}    3    msg=PayloadLength must be 3

    ${b1}=    Read SX1278 Reg    0x00
    ${b2}=    Read SX1278 Reg    0x00
    ${b3}=    Read SX1278 Reg    0x00
    Should Be Equal As Integers    ${b1}    0x11    msg=injected byte 0 mismatch
    Should Be Equal As Integers    ${b2}    0x22    msg=injected byte 1 mismatch
    Should Be Equal As Integers    ${b3}    0x33    msg=injected byte 2 mismatch

# ── TC-8 ─────────────────────────────────────────────────────────
ForceFailCount Suppresses TX And Medium Delivery
    [Documentation]    Writing N to +0xFF8 causes the next N TX operations to be
    ...                silently dropped: TxDone is NOT set and FrameSent is NOT
    ...                called, so Pramukh receives nothing.
    ...                txCount still increments to reflect the attempt.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    # Arm forced fail for the next 1 TX
    Execute Command    sysbus WriteDoubleWord ${MAGIC_FORCEFL}   1

    Transmit SX1278 Frame    0x01    0x02    0x03    0x04

    # TxDone must NOT be set
    ${irq_g}=    Read SX1278 IrqFlags
    Should Be Equal As Integers    ${irq_g}    0
    ...    msg=TxDone must not be set when forceFailCount consumes the TX

    # txCount still increments
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${MAGIC_TXCOUNT}
    Should Be Equal As Integers    ${raw.strip()}    1
    ...    msg=txCount must be 1 even for a forced-fail TX

    # Pramukh must not receive anything
    Switch To Machine    SWM-Pramukh-MC
    ${irq_p}=    Read SX1278 IrqFlags
    Should Be Equal As Integers    ${irq_p}    0
    ...    msg=Pramukh must not receive when TX is force-failed

# ── TC-9 ─────────────────────────────────────────────────────────
Back-To-Back Frames Queued And Read Correctly
    [Documentation]    Two consecutive TX from Gagan enqueue two frames on Pramukh.
    ...                PromoteRxHead triggers immediately when the last byte of
    ...                frame-1 is popped, updating PayloadLength for frame-2 before
    ...                the next PayloadLength read.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SX1278 Frame    0x01    0x02    0x03    0x04
    Transmit SX1278 Frame    0x05    0x06    0x07    0x08

    Switch To Machine    SWM-Pramukh-MC
    # Read first frame
    ${len1}=    Read SX1278 Reg    0x22
    Should Be Equal As Integers    ${len1}    4    msg=PayloadLength of frame-1 must be 4
    ${f1b1}=    Read SX1278 Reg    0x00
    ${f1b2}=    Read SX1278 Reg    0x00
    ${f1b3}=    Read SX1278 Reg    0x00
    ${f1b4}=    Read SX1278 Reg    0x00
    Should Be Equal As Integers    ${f1b1}    0x01    msg=frame-1 byte 0 mismatch
    Should Be Equal As Integers    ${f1b2}    0x02    msg=frame-1 byte 1 mismatch
    Should Be Equal As Integers    ${f1b3}    0x03    msg=frame-1 byte 2 mismatch
    Should Be Equal As Integers    ${f1b4}    0x04    msg=frame-1 byte 3 mismatch

    # After reading the last byte of frame-1, frame-2 is promoted immediately.
    # PayloadLength is updated before this next read.
    ${len2}=    Read SX1278 Reg    0x22
    Should Be Equal As Integers    ${len2}    4    msg=PayloadLength of frame-2 must be 4
    ${f2b1}=    Read SX1278 Reg    0x00
    ${f2b2}=    Read SX1278 Reg    0x00
    ${f2b3}=    Read SX1278 Reg    0x00
    ${f2b4}=    Read SX1278 Reg    0x00
    Should Be Equal As Integers    ${f2b1}    0x05    msg=frame-2 byte 0 mismatch
    Should Be Equal As Integers    ${f2b2}    0x06    msg=frame-2 byte 1 mismatch
    Should Be Equal As Integers    ${f2b3}    0x07    msg=frame-2 byte 2 mismatch
    Should Be Equal As Integers    ${f2b4}    0x08    msg=frame-2 byte 3 mismatch

*** Keywords ***

Prepare Emulation
    Reset Emulation

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC}
    Execute Command    logLevel 0

Write SX1278 Reg
    [Documentation]    Single SX1278 register write: cmd_byte = reg | 0x80.
    [Arguments]    ${reg}    ${val}
    ${w0}=    Evaluate    "0x{:08X}".format((${val} << 8) | (${reg} | 0x80))
    Execute Command    sysbus WriteDoubleWord ${W0_REG} ${w0}
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}

Read SX1278 Reg
    [Documentation]    Single SX1278 register read: cmd_byte = reg & 0x7F.
    ...                Returns the 8-bit value placed at W0[15:8] by the model.
    [Arguments]    ${reg}
    ${w0}=    Evaluate    "0x{:08X}".format(${reg} & 0x7F)
    Execute Command    sysbus WriteDoubleWord ${W0_REG} ${w0}
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${W0_REG}
    ${val}=    Evaluate    (int('${raw.strip()}', 16) >> 8) & 0xFF
    [Return]    ${val}

Read SX1278 IrqFlags
    [Documentation]    Reads RegIrqFlags (0x12) and returns the byte value.
    ${irq}=    Read SX1278 Reg    0x12
    [Return]    ${irq}

Transmit SX1278 Frame
    [Documentation]    Drives the SX1278 TX sequence for a 4-byte frame:
    ...                  write_reg(0x0D, 0)     — reset FifoAddrPtr
    ...                  write_reg(0x00, b) × 4 — RegFifo byte writes
    ...                  write_reg(0x22, 4)     — RegPayloadLength
    ...                  write_reg(0x01, 0x83)  — RegOpMode TX → DispatchTx()
    [Arguments]    ${b1}    ${b2}    ${b3}    ${b4}
    # Reset FifoAddrPtr: cmd=0x8D, val=0x00 → W0 = 0x0000008D
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x0000008D
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # RegFifo byte writes: cmd=0x80, W0 = (b << 8) | 0x80
    ${w0_b1}=    Evaluate    "0x{:08X}".format((${b1} << 8) | 0x80)
    Execute Command    sysbus WriteDoubleWord ${W0_REG} ${w0_b1}
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    ${w0_b2}=    Evaluate    "0x{:08X}".format((${b2} << 8) | 0x80)
    Execute Command    sysbus WriteDoubleWord ${W0_REG} ${w0_b2}
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    ${w0_b3}=    Evaluate    "0x{:08X}".format((${b3} << 8) | 0x80)
    Execute Command    sysbus WriteDoubleWord ${W0_REG} ${w0_b3}
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    ${w0_b4}=    Evaluate    "0x{:08X}".format((${b4} << 8) | 0x80)
    Execute Command    sysbus WriteDoubleWord ${W0_REG} ${w0_b4}
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # RegPayloadLength = 4: cmd=0xA2, val=0x04 → W0 = 0x000004A2
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x000004A2
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # RegOpMode TX: cmd=0x81, val=0x83 → W0 = 0x00008381
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00008381
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
