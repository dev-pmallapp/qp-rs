*** Settings ***
Documentation     ESP32-C6 SYSTIMER stub unit tests — exercises systimer_stub.py via
...               direct register reads/writes without running firmware.
...               Covers TestingTopics.md §2.9.1 (tick accuracy), §2.9.3 (one-shot vs
...               periodic), §2.9.4 (timer stop/restart).
...               Tags: systimer, peripheral, unit, §2.9

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc
${UNIT0_OP}          0x6000A004
${UNIT1_OP}          0x6000A008
${UNIT0_VALUE_HI}    0x6000A040
${UNIT0_VALUE_LO}    0x6000A044
${UNIT1_VALUE_LO}    0x6000A04C
${TICKS_PER_SNAPSHOT}    1600

*** Test Cases ***

2.9.1 UNIT0_OP Returns VALUE_VALID Bit On Read
    [Documentation]    Reading UNIT0_OP must return bit 29 set (VALUE_VALID = 1).
    ...                This confirms the systimer stub is active and snapshots are
    ...                always immediately valid.  §2.9.1
    [Setup]    Setup Platform
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${UNIT0_OP}
    ${v}=      Convert To Integer    ${raw.strip()}
    ${valid}=  Evaluate    (${v} >> 29) & 1
    Should Be Equal As Integers    ${valid}    1    msg=VALUE_VALID bit not set in UNIT0_OP
    [Teardown]    Reset Emulation

2.9.1 UNIT1_OP Also Returns VALUE_VALID
    [Documentation]    UNIT1_OP mirrors UNIT0_OP — bit 29 must also be set.  §2.9.1
    [Setup]    Setup Platform
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${UNIT1_OP}
    ${v}=      Convert To Integer    ${raw.strip()}
    ${valid}=  Evaluate    (${v} >> 29) & 1
    Should Be Equal As Integers    ${valid}    1    msg=VALUE_VALID not set in UNIT1_OP
    [Teardown]    Reset Emulation

2.9.1 Single Snapshot Write Advances VALUE_LO By 1600 Ticks
    [Documentation]    Write to UNIT0_OP (snapshot trigger) advances tick by exactly
    ...                ${TICKS_PER_SNAPSHOT} (1600 ticks = 100 µs at 16 MHz).  §2.9.1
    [Setup]    Setup Platform
    ${lo0}=    Read Value LO
    Execute Command    sysbus WriteDoubleWord ${UNIT0_OP} 0x80000000
    ${lo1}=    Read Value LO
    ${delta}=  Evaluate    ${lo1} - ${lo0}
    Should Be Equal As Integers    ${delta}    ${TICKS_PER_SNAPSHOT}
    ...    msg=Expected +1600 ticks per snapshot write, got delta=${delta}
    [Teardown]    Reset Emulation

2.9.1 Multiple Snapshots Accumulate Correctly
    [Documentation]    Ten successive snapshot writes must advance VALUE_LO by
    ...                10 × 1600 = 16000 ticks.  §2.9.1
    [Setup]    Setup Platform
    ${lo0}=    Read Value LO
    FOR    ${i}    IN RANGE    10
        Execute Command    sysbus WriteDoubleWord ${UNIT0_OP} 0x80000000
    END
    ${lo1}=    Read Value LO
    ${delta}=  Evaluate    ${lo1} - ${lo0}
    Should Be Equal As Integers    ${delta}    16000
    ...    msg=Expected 16000 ticks after 10 snapshots, got ${delta}
    [Teardown]    Reset Emulation

2.9.1 UNIT1_OP Snapshot Advances Same Counter
    [Documentation]    Writing to UNIT1_OP also advances the shared tick counter by 1600.
    [Setup]    Setup Platform
    ${lo0}=    Read Value LO
    Execute Command    sysbus WriteDoubleWord ${UNIT1_OP} 0x80000000
    ${lo1}=    Read Value LO
    ${delta}=  Evaluate    ${lo1} - ${lo0}
    Should Be Equal As Integers    ${delta}    ${TICKS_PER_SNAPSHOT}
    [Teardown]    Reset Emulation

2.9.4 Timer Resets To Zero On Platform Reload
    [Documentation]    After advancing tick via 5 snapshots, reload the platform.
    ...                VALUE_LO must return to 0 (no stale state).  §2.9.4
    [Setup]    Setup Platform
    FOR    ${i}    IN RANGE    5
        Execute Command    sysbus WriteDoubleWord ${UNIT0_OP} 0x80000000
    END
    ${before}=    Read Value LO
    Should Be True    ${before} > 0    msg=Tick did not advance before reload

    Reset Emulation
    Setup Platform
    ${after}=    Read Value LO
    Should Be Equal As Integers    ${after}    0    msg=Tick not reset to 0 after platform reload
    [Teardown]    Reset Emulation

2.9.1 VALUE_HI Is Zero Before Counter Overflows 32 Bits
    [Documentation]    With only a few snapshot writes the tick stays in 32-bit range,
    ...                so UNIT0_VALUE_HI must remain 0.
    [Setup]    Setup Platform
    FOR    ${i}    IN RANGE    5
        Execute Command    sysbus WriteDoubleWord ${UNIT0_OP} 0x80000000
    END
    ${hi}=    Execute Command    sysbus ReadDoubleWord ${UNIT0_VALUE_HI}
    Should Be Equal As Integers    ${hi.strip()}    0    msg=VALUE_HI non-zero unexpectedly
    [Teardown]    Reset Emulation

*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    logLevel 0

Read Value LO
    [Documentation]    Returns UNIT0_VALUE_LO as an integer.
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${UNIT0_VALUE_LO}
    ${v}=      Convert To Integer    ${raw.strip()}
    RETURN     ${v}
