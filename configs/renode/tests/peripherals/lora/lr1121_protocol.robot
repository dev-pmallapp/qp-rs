*** Settings ***
Documentation     LR1121Radio peripheral model unit tests.
...               Tests drive SPI registers directly — no firmware required.
...               Each test case exercises one protocol path of LR1121Radio.cs
...               and verifies the state machine and wireless-medium delivery.

Suite Setup       Prepare Emulation
Suite Teardown    Reset Emulation
Test Teardown     Reset Emulation

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC}            ${CURDIR}${/}test_lora_swm_platform.resc

# GPSPI2 register addresses (spi2 base = 0x60081000)
${CMD_REG}         0x60081000
${W0_REG}          0x60081098
${W1_REG}          0x6008109C

# Single value that triggers a GPSPI transaction (USR bit = bit 24)
${CMD_TRIGGER}     0x01000000

# Expected W0 values after GetIrqStatus tx3:
#   W0 = ReverseBytes(irqFlags)
#   TxDone (0x08) → ReverseBytes(0x00000008) = 0x08000000
#   RxDone (0x02) → ReverseBytes(0x00000002) = 0x02000000
${TXDONE_W0}       0x08000000
${RXDONE_W0}       0x02000000

*** Test Cases ***

# ── TC-1 ─────────────────────────────────────────────────────────
TX Path Sets TxDone IRQ
    [Documentation]    WriteBuffer (3 transactions) + SetTx (1 transaction) on Gagan.
    ...                GetIrqStatus must return TxDone (bit 3 = 0x08).
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SPI Payload    0x04030201

    ${irq}=    Get IRQ Status
    Should Be Equal As Integers    ${irq}    ${TXDONE_W0}
    ...    msg=TxDone (0x08000000) not set after SetTx

# ── TC-2 ─────────────────────────────────────────────────────────
Gagan TX Delivers RxDone To Pramukh
    [Documentation]    When Gagan executes WriteBuffer + SetTx the wireless medium
    ...                calls Pramukh's ReceiveFrame synchronously.
    ...                Pramukh's GetIrqStatus must return RxDone (bit 1 = 0x02).
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SPI Payload    0x04030201

    Switch To Machine    SWM-Pramukh-MC
    ${irq}=    Get IRQ Status
    Should Be Equal As Integers    ${irq}    ${RXDONE_W0}
    ...    msg=RxDone (0x02000000) not set on Pramukh after Gagan TX

# ── TC-3 ─────────────────────────────────────────────────────────
ClearIrqStatus Clears TxDone
    [Documentation]    After SetTx the model sets TxDone (0x08).
    ...                ClearIrqStatus with mask 0x00000008 (two transactions)
    ...                must clear it so the next GetIrqStatus returns 0.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SPI Payload    0x04030201

    # ClearIrqStatus: T1 = cmd [0x01, 0x15], T2 = mask [0x00, 0x00, 0x00, 0x08]
    # T1: W0 = [0x01, 0x15, 0x00, 0x00] → 0x00001501
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00001501
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # T2: mask bytes[0..3] = [0x00, 0x00, 0x00, 0x08]
    #     W0[bits 24-31] = 0x08 → W0 = 0x08000000
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x08000000
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}

    ${irq}=    Get IRQ Status
    Should Be Equal As Integers    ${irq}    0
    ...    msg=TxDone bit must be 0 after ClearIrqStatus

# ── TC-4 ─────────────────────────────────────────────────────────
GetRxBufferStatus Returns Positive Length After RX
    [Documentation]    After Gagan TX, Pramukh's GetRxBufferStatus (3 transactions)
    ...                must return a non-zero rx_len in W0[7:0].
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SPI Payload    0x04030201

    Switch To Machine    SWM-Pramukh-MC
    # GetRxBufferStatus cmd: W0 = [0x01, 0x0D, ...] = 0x00000D01
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000D01
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # Dummy
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000000
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # Response — model fills W0 with rx_len
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000000
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}

    ${raw}=    Execute Command    sysbus ReadDoubleWord ${W0_REG}
    ${rx_len}=    Evaluate    int('${raw.strip()}', 16) & 0xFF
    Should Be True    ${rx_len} > 0    msg=rx_len must be > 0 after Gagan TX

# ── TC-5 ─────────────────────────────────────────────────────────
ReadBuffer Recovers First Payload Word
    [Documentation]    Gagan writes 0x04030201 into W0 as the WriteBuffer payload.
    ...                After Pramukh's ReadBuffer (4 transactions), W0 must equal
    ...                0x04030201 — confirming the frame bytes traverse the medium.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SPI Payload    0x04030201

    Switch To Machine    SWM-Pramukh-MC
    # ReadBuffer cmd: W0 = [0x01, 0x08, ...] = 0x00000801
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000801
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # Offset byte = 0
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000000
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # Dummy
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000000
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # Data — model fills W registers with rxBuffer bytes
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000000
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}

    ${raw}=    Execute Command    sysbus ReadDoubleWord ${W0_REG}
    Should Be Equal As Integers    ${raw.strip()}    0x04030201
    ...    msg=ReadBuffer W0 must equal Gagan's payload word

# ── TC-6 ─────────────────────────────────────────────────────────
Out Of Range Node Does Not Receive
    [Documentation]    SWM-OOR-Node is at (300,0,0) — 300 units from Gagan.
    ...                The medium range is 200 units so ReceiveFrame must NOT be
    ...                called on the OOR node: its irqFlags must stay 0.
    Setup Platform

    Switch To Machine    SWM-Gagan-OHT
    Transmit SPI Payload    0x04030201

    Switch To Machine    SWM-OOR-Node
    ${irq}=    Get IRQ Status
    Should Be Equal As Integers    ${irq}    0
    ...    msg=Out-of-range node must not receive the frame

*** Keywords ***

Prepare Emulation
    Reset Emulation

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC}
    Execute Command    logLevel 0

Transmit SPI Payload
    [Documentation]    Sends WriteBuffer (3 txns) + SetTx (1 txn) to the current
    ...                machine with the given 4-byte W0 payload word.
    [Arguments]    ${w0_payload}
    # T1 — WriteBuffer cmd: bytes = [0x01, 0x09, ...] → W0 = 0x00000901
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000901
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # T2 — offset byte = 0x00
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000000
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # T3 — payload data: W0 = first 4 bytes, rest = 0 (W1..W15 cleared by Reset)
    Execute Command    sysbus WriteDoubleWord ${W0_REG} ${w0_payload}
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # T4 — SetTx cmd: bytes = [0x02, 0x0A, ...] → W0 = 0x00000A02
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000A02
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}

Get IRQ Status
    [Documentation]    Runs the 3-transaction GetIrqStatus protocol on the current
    ...                machine and returns the raw W0 value (= ReverseBytes(irqFlags)).
    # T1 — GetIrqStatus cmd: [0x01, 0x14] → W0 = 0x00001401
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00001401
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # T2 — dummy byte
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000000
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    # T3 — response: model fills W0 with ReverseBytes(irqFlags)
    Execute Command    sysbus WriteDoubleWord ${W0_REG} 0x00000000
    Execute Command    sysbus WriteDoubleWord ${CMD_REG} ${CMD_TRIGGER}
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${W0_REG}
    [Return]    ${raw.strip()}
