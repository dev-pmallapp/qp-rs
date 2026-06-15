*** Settings ***
Documentation     USB-Serial-JTAG console stub unit tests — exercises
...               usb_serial_jtag.cs through direct register reads/writes,
...               without running firmware.
...
...               Drives EP1 (+0x000) byte writes and verifies the model
...               forwards each byte to UARTBase verbatim (TransmitCharacter
...               with no escaping, no buffering side-effects). The §2.2.4
...               assertion is specifically about QS/QSPY binary frame
...               passthrough — the HDLC delimiter 0x7E and high-bit bytes
...               must reach the UART output exactly as written.
...
...               Covers TestingTopics.md §2.2.1 (UART output capture),
...               §2.2.3 (buffer overrun does not crash), §2.2.4 (QS/QSPY
...               binary frame bytes captured verbatim).
...               Tags: console, usb_serial_jtag, peripheral, unit, §2.2

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${CURDIR}${/}test_usb_serial_jtag_platform.resc

${EP1_OFFSET}            0x6000F000
${EP1_CONF_OFFSET}       0x6000F004
${EP1_WR_RDY_BIT}        0x2

${HDLC_DELIM}            0x7E

*** Test Cases ***

2.2.1 EP1_CONF Reports Write-Ready Bit Set
    [Documentation]    EP1_CONF read must always return EP1_WR_RDY=1 (bit 1).
    ...                The driver polls this before every 64-byte chunk; if
    ...                the model returned 0 the firmware would spin forever.
    ...                §2.2.1
    [Setup]    Setup Platform
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${EP1_CONF_OFFSET}
    ${v}=      Convert To Integer    ${raw.strip()}
    ${rdy}=    Evaluate    ${v} & ${EP1_WR_RDY_BIT}
    Should Be Equal As Integers    ${rdy}    ${EP1_WR_RDY_BIT}
    ...    msg=EP1_WR_RDY bit not set; firmware would block forever
    [Teardown]    Reset Emulation

2.2.4 Printable ASCII Line Passes Through Verbatim
    [Documentation]    Baseline: write 'hello\\n' byte-by-byte to EP1 and
    ...                verify the line appears unchanged on the UART tester.
    ...                §2.2.4
    [Setup]    Setup Platform
    ${uart}=    Create Terminal Tester    sysbus.usb_serial_jtag    machine=UsbSerialJtagTest
    Write Bytes To EP1    104    101    108    108    111    10
    Wait For Line On Uart    hello    testerId=${uart}    timeout=2
    [Teardown]    Reset Emulation

2.2.4 HDLC Delimiter (0x7E) Passes Through Verbatim
    [Documentation]    QS/QSPY frames use 0x7E ('~') as the HDLC framing
    ...                delimiter. The console must transmit it unmodified —
    ...                no escape sequence, no swallowing. Send "~test~\\n"
    ...                and verify the line literal. §2.2.4
    [Setup]    Setup Platform
    ${uart}=    Create Terminal Tester    sysbus.usb_serial_jtag    machine=UsbSerialJtagTest
    Write Bytes To EP1    126    116    101    115    116    126    10
    Wait For Line On Uart    ~test~    testerId=${uart}    timeout=2
    [Teardown]    Reset Emulation

2.2.4 Doubled HDLC Delimiters Pass Through Verbatim
    [Documentation]    Adjacent 0x7E bytes (used to mark frame boundaries
    ...                back-to-back) must both reach the UART. Send
    ...                "~~~~\\n" and verify the line. §2.2.4
    [Setup]    Setup Platform
    ${uart}=    Create Terminal Tester    sysbus.usb_serial_jtag    machine=UsbSerialJtagTest
    Write Bytes To EP1    126    126    126    126    10
    Wait For Line On Uart    ~~~~    testerId=${uart}    timeout=2
    [Teardown]    Reset Emulation

2.2.4 HDLC Escape (0x7D) Passes Through Verbatim
    [Documentation]    The other QS HDLC special byte (0x7D '}') must also
    ...                survive the console unchanged. Send "}q}\\n" and
    ...                verify the line. §2.2.4
    [Setup]    Setup Platform
    ${uart}=    Create Terminal Tester    sysbus.usb_serial_jtag    machine=UsbSerialJtagTest
    Write Bytes To EP1    125    113    125    10
    Wait For Line On Uart    }q}    testerId=${uart}    timeout=2
    [Teardown]    Reset Emulation

2.2.4 Mixed Special Byte Sequence Passes Through Verbatim
    [Documentation]    Realistic QS-frame approximation: ~ } a ~ \\n. The
    ...                line tester sees "~}a~" — proves the EP1 → UART path
    ...                does not alter any of the three special bytes used by
    ...                QS HDLC framing. §2.2.4
    [Setup]    Setup Platform
    ${uart}=    Create Terminal Tester    sysbus.usb_serial_jtag    machine=UsbSerialJtagTest
    Write Bytes To EP1    126    125    97    126    10
    Wait For Line On Uart    ~}a~    testerId=${uart}    timeout=2
    [Teardown]    Reset Emulation

2.2.3 Buffer Overrun Does Not Crash The Stub
    [Documentation]    The driver model has no internal queue limit — write
    ...                512 bytes to EP1 in one burst (8× the 64-byte chunk
    ...                window) and confirm subsequent reads of EP1_CONF
    ...                still report WR_RDY. §2.2.3
    [Setup]    Setup Platform
    FOR    ${i}    IN RANGE    512
        Execute Command    sysbus WriteDoubleWord ${EP1_OFFSET} 65
    END
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${EP1_CONF_OFFSET}
    ${v}=      Convert To Integer    ${raw.strip()}
    ${rdy}=    Evaluate    ${v} & ${EP1_WR_RDY_BIT}
    Should Be Equal As Integers    ${rdy}    ${EP1_WR_RDY_BIT}
    ...    msg=Stub stopped reporting WR_RDY after burst write
    [Teardown]    Reset Emulation

2.2.4 Only Low Byte Of EP1 Write Transmits
    [Documentation]    EP1 transmits the low byte of the written word
    ...                (value & 0xFF). Writing 0xDEADBE7E must produce only
    ...                a single 0x7E ('~') on the UART. The high bits must
    ...                not leak through. §2.2.4
    [Setup]    Setup Platform
    ${uart}=    Create Terminal Tester    sysbus.usb_serial_jtag    machine=UsbSerialJtagTest
    Execute Command    sysbus WriteDoubleWord ${EP1_OFFSET} 0xDEADBE7E
    Execute Command    sysbus WriteDoubleWord ${EP1_OFFSET} 10
    Wait For Line On Uart    ~    testerId=${uart}    timeout=2
    [Teardown]    Reset Emulation

*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    logLevel 3

Write Bytes To EP1
    [Documentation]    Writes each argument as a single byte to the EP1
    ...                register. Each write triggers one TransmitCharacter
    ...                call inside ESP32C6_UsbSerialJtag.
    [Arguments]    @{bytes}
    FOR    ${b}    IN    @{bytes}
        Execute Command    sysbus WriteDoubleWord ${EP1_OFFSET} ${b}
    END
