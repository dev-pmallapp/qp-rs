*** Settings ***
Documentation     SPI-NOR flash stub unit tests — exercises flash_stub.py
...               through direct register reads/writes, without running
...               firmware.
...
...               Stub base: 0x60020000 (test-only — production firmware on
...               ESP32-C6 uses esp-storage internal flash, not this model).
...               Content window: 16 KB (4 sectors × 4 KB). Control window
...               begins at +0x4000.
...
...               Covers TestingTopics.md §2.3.1 (erase/write cycle), §2.3.2
...               (boundary conditions — first / last page, zero-length),
...               §2.3.3 (persistence semantics across platform reload),
...               §2.3.4 (JEDEC ID), §2.3.5 (wear counter and write protect).
...               Tags: flash, storage, peripheral, unit, §2.3

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${CURDIR}${/}test_flash_platform.resc

${FLASH_BASE}            0x60020000
${SECTOR_SIZE}           4096
${NUM_SECTORS}           4

${OFF_SECTOR0_FIRST}     0x60020000
${OFF_SECTOR0_LAST}      0x60020FFC
${OFF_SECTOR3_FIRST}     0x60023000
${OFF_SECTOR3_LAST}      0x60023FFC

${REG_STATUS}            0x60024000
${REG_JEDEC_ID}          0x60024004
${REG_ERASE_SECTOR}      0x60024008
${REG_WRITE_COUNT}       0x6002400C
${REG_SECTOR_SIZE}       0x60024010
${REG_NUM_SECTORS}       0x60024014
${REG_WRITE_PROTECT}     0x60024018

${ERASED_WORD}           0xFFFFFFFF
${JEDEC_EXPECTED}        0x00EF4014

*** Test Cases ***

2.3.4 JEDEC ID Reports Vendor / Memory Type / Capacity
    [Documentation]    JEDEC_ID register returns the constant
    ...                vendor:memtype:cap triple modelled on Winbond W25Q80:
    ...                0x00 || 0xEF || 0x40 || 0x14. §2.3.4
    [Setup]    Setup Platform
    ${id}=    Read Register    ${REG_JEDEC_ID}
    Should Be Equal As Integers    ${id}    ${JEDEC_EXPECTED}
    ...    msg=JEDEC_ID mismatch — got ${id}
    [Teardown]    Reset Emulation

2.3.4 STATUS Register Always Clears Busy Bit
    [Documentation]    Without dynamic timing, the simulated NOR is never
    ...                busy — STATUS must always read 0. §2.3.4
    [Setup]    Setup Platform
    ${s}=    Read Register    ${REG_STATUS}
    Should Be Equal As Integers    ${s}    0    msg=STATUS reports busy unexpectedly
    [Teardown]    Reset Emulation

2.3.4 Geometry Registers Report Configured Sector Size And Count
    [Documentation]    SECTOR_SIZE and NUM_SECTORS must match the constants
    ...                used by the test variables. §2.3.4
    [Setup]    Setup Platform
    ${ss}=    Read Register    ${REG_SECTOR_SIZE}
    Should Be Equal As Integers    ${ss}    ${SECTOR_SIZE}
    ${ns}=    Read Register    ${REG_NUM_SECTORS}
    Should Be Equal As Integers    ${ns}    ${NUM_SECTORS}
    [Teardown]    Reset Emulation

2.3.1 Sector Is Erased At Init (Reads Return 0xFFFFFFFF)
    [Documentation]    A fresh flash is all-erased. §2.3.1
    [Setup]    Setup Platform
    ${w}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${w}    ${ERASED_WORD}
    ...    msg=First word of sector 0 not erased at init
    [Teardown]    Reset Emulation

2.3.1 Write Then Read Round-Trips A Pattern
    [Documentation]    Write 0xDEADBEEF to sector 0 base, read it back
    ...                identically. §2.3.1
    [Setup]    Setup Platform
    Write Register    ${OFF_SECTOR0_FIRST}    0xDEADBEEF
    ${w}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${w}    0xDEADBEEF    msg=Pattern not preserved
    [Teardown]    Reset Emulation

2.3.1 SPI-NOR Program Cannot Set Bits Without Erase
    [Documentation]    Programming AND-merges bits (program-to-zero). Writing
    ...                0xAAAAAAAA after 0x55555555 yields 0x00000000 because
    ...                no bit position is HIGH in both. §2.3.1
    [Setup]    Setup Platform
    Write Register    ${OFF_SECTOR0_FIRST}    0x55555555
    Write Register    ${OFF_SECTOR0_FIRST}    0xAAAAAAAA
    ${w}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${w}    0
    ...    msg=AND-merged write produced ${w} (expected 0)
    [Teardown]    Reset Emulation

2.3.1 Erase Sector Restores 0xFFFFFFFF
    [Documentation]    After programming sector 0 to 0, erasing sector 0
    ...                must restore the entire 4 KB sector to 0xFFFFFFFF.
    ...                §2.3.1
    [Setup]    Setup Platform
    Write Register    ${OFF_SECTOR0_FIRST}    0x12345678
    Write Register    ${OFF_SECTOR0_LAST}    0xABCDEF01
    Erase Sector    0
    ${first}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${first}    ${ERASED_WORD}
    ${last}=    Read Register    ${OFF_SECTOR0_LAST}
    Should Be Equal As Integers    ${last}    ${ERASED_WORD}
    [Teardown]    Reset Emulation

2.3.1 Erase Sector Leaves Other Sectors Untouched
    [Documentation]    Erasing sector 1 must not affect sector 0 contents.
    ...                §2.3.1
    [Setup]    Setup Platform
    Write Register    ${OFF_SECTOR0_FIRST}    0xCAFEF00D
    Erase Sector    1
    ${w}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${w}    0xCAFEF00D
    ...    msg=Sector 0 corrupted by sector-1 erase
    [Teardown]    Reset Emulation

2.3.2 First-Page Boundary Is Writable
    [Documentation]    Offset 0 is the first writable word — confirm a write
    ...                takes effect. §2.3.2
    [Setup]    Setup Platform
    Write Register    ${OFF_SECTOR0_FIRST}    0x11223344
    ${w}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${w}    0x11223344
    [Teardown]    Reset Emulation

2.3.2 Last-Page Boundary Is Writable
    [Documentation]    Offset 0x3FFC is the last word in the 16 KB content
    ...                window. Writing it must take effect; the control
    ...                registers above 0x4000 must be unaffected. §2.3.2
    [Setup]    Setup Platform
    Write Register    ${OFF_SECTOR3_LAST}    0x55667788
    ${last}=    Read Register    ${OFF_SECTOR3_LAST}
    Should Be Equal As Integers    ${last}    0x55667788
    ${jedec}=    Read Register    ${REG_JEDEC_ID}
    Should Be Equal As Integers    ${jedec}    ${JEDEC_EXPECTED}
    ...    msg=Last-page write corrupted control register
    [Teardown]    Reset Emulation

2.3.2 Misaligned Content Read Returns Zero
    [Documentation]    The model accepts DoubleWord-aligned reads only — a
    ...                read at +0x0001 returns 0 rather than tearing a word.
    ...                §2.3.2
    [Setup]    Setup Platform
    ${raw}=    Execute Command    sysbus ReadDoubleWord 0x60020001
    Should Be Equal As Integers    ${raw.strip()}    0
    [Teardown]    Reset Emulation

2.3.3 Content Survives Across Sector-Internal Operations
    [Documentation]    Programming sector 2 then reading it back without an
    ...                intervening erase or platform reload must preserve the
    ...                exact written words (no spurious clearing). §2.3.3
    [Setup]    Setup Platform
    Write Register    0x60022000    0xAA55AA55
    Write Register    0x60022004    0x12345678
    ${w0}=    Read Register    0x60022000
    Should Be Equal As Integers    ${w0}    0xAA55AA55
    ${w1}=    Read Register    0x60022004
    Should Be Equal As Integers    ${w1}    0x12345678
    [Teardown]    Reset Emulation

2.3.3 Stub Resets To Erased State On Platform Reload
    [Documentation]    Write a pattern, reload the emulation — the same
    ...                offset must read back 0xFFFFFFFF (no stale state).
    ...                §2.3.3
    [Setup]    Setup Platform
    Write Register    ${OFF_SECTOR0_FIRST}    0x99887766
    Reset Emulation
    Setup Platform
    ${w}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${w}    ${ERASED_WORD}
    ...    msg=Stale content survived platform reload
    [Teardown]    Reset Emulation

2.3.5 WRITE_COUNT Advances Four Bytes Per DoubleWord Write
    [Documentation]    Each successful content write must add 4 to
    ...                WRITE_COUNT. Five writes ⇒ count = 20. §2.3.5
    [Setup]    Setup Platform
    FOR    ${i}    IN RANGE    5
        Write Register    ${OFF_SECTOR0_FIRST}    0xFFFFFFFF
    END
    ${c}=    Read Register    ${REG_WRITE_COUNT}
    Should Be Equal As Integers    ${c}    20    msg=WRITE_COUNT did not advance correctly
    [Teardown]    Reset Emulation

2.3.5 Write Protect Blocks Programming
    [Documentation]    With WRITE_PROTECT=1, content writes are no-ops. The
    ...                target word stays at 0xFFFFFFFF. §2.3.5
    [Setup]    Setup Platform
    Write Register    ${REG_WRITE_PROTECT}    1
    Write Register    ${OFF_SECTOR0_FIRST}    0xDEADBEEF
    ${w}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${w}    ${ERASED_WORD}
    ...    msg=Write took effect despite WRITE_PROTECT
    [Teardown]    Reset Emulation

2.3.5 Write Protect Blocks Sector Erase
    [Documentation]    With WRITE_PROTECT=1, ERASE_SECTOR is also a no-op —
    ...                programmed content survives an attempted erase. §2.3.5
    [Setup]    Setup Platform
    Write Register    ${OFF_SECTOR0_FIRST}    0x12345678
    Write Register    ${REG_WRITE_PROTECT}    1
    Erase Sector    0
    ${w}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${w}    0x12345678
    ...    msg=Erase succeeded despite WRITE_PROTECT
    [Teardown]    Reset Emulation

2.3.5 Write Protect Releases After Clearing
    [Documentation]    Clearing WRITE_PROTECT to 0 must restore writability.
    ...                §2.3.5
    [Setup]    Setup Platform
    Write Register    ${REG_WRITE_PROTECT}    1
    Write Register    ${REG_WRITE_PROTECT}    0
    Write Register    ${OFF_SECTOR0_FIRST}    0x12345678
    ${w}=    Read Register    ${OFF_SECTOR0_FIRST}
    Should Be Equal As Integers    ${w}    0x12345678
    [Teardown]    Reset Emulation

*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    logLevel 0

Read Register
    [Arguments]    ${addr}
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${addr}
    ${v}=      Convert To Integer    ${raw.strip()}
    RETURN     ${v}

Write Register
    [Arguments]    ${addr}    ${value}
    Execute Command    sysbus WriteDoubleWord ${addr} ${value}

Erase Sector
    [Arguments]    ${sector}
    Execute Command    sysbus WriteDoubleWord ${REG_ERASE_SECTOR} ${sector}
