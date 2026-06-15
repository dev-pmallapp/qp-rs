*** Settings ***
Documentation     HC-SR04 ultrasonic sensor stub unit tests — exercises
...               ultrasonic_hcsr04.py through direct GPIO register reads/writes,
...               without running firmware.
...
...               Drives the stub's trigger/echo protocol:
...                 - GPIO_OUT_W1TS (+0x008) bit 6 → trigger HIGH
...                 - GPIO_OUT_W1TC (+0x00C) bit 6 → trigger LOW (starts echo cycle)
...                 - GPIO_IN       (+0x03C) bit 7 → echo state
...               Distance is configured via the +0xFF0 magic register, which the
...               stub uses to derive PULSE_READS (echo HIGH-state read count).
...
...               Covers TestingTopics.md §2.7.1 (pulse-timing → distance),
...               §2.7.2 (empty tank — max distance), §2.7.3 (full tank — min
...               distance), §2.7.4 (no-echo timeout when trigger absent).
...               Tags: ultrasonic, sensor, peripheral, unit, §2.7

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc
${GPIO_OUT_W1TS}     0x60091008
${GPIO_OUT_W1TC}     0x6009100C
${GPIO_IN}           0x6009103C
# +0xFF0 magic register: base 0x60091000 + 0xFF0 = 0x60091FF0
${GPIO_DISTANCE_MAGIC}    0x60091FF0
${TRIGGER_BIT}       0x40
${ECHO_BIT}          0x80
# Max iterations for the echo-HIGH count loop. Caps run-time on a runaway stub.
${MAX_HIGH_READS}    400

*** Test Cases ***

2.7.1 Echo Pulse Width Reflects Default Distance (800 mm)
    [Documentation]    With the stub at its default 800 mm distance, after a
    ...                trigger pulse the echo pin must stay HIGH for the count
    ...                derived from pulse_reads(800) = max(2, 800*2000/(343*100))
    ...                = ~46 reads. We assert ≥10 (well within the floor) so
    ...                small calibration deltas in the formula do not break the
    ...                test. §2.7.1
    [Setup]    Setup Platform
    Send Trigger Pulse
    ${high_count}=    Count Echo High Reads
    Should Be True    ${high_count} >= 10
    ...    msg=Expected echo HIGH for ≥10 reads at default distance, got ${high_count}
    [Teardown]    Reset Emulation

2.7.1 Distance Increase Lengthens Echo HIGH Window
    [Documentation]    Inject 1600 mm. pulse_reads(1600) ≈ 93. Echo HIGH read
    ...                count must be greater than the default 800-mm baseline.
    ...                §2.7.1
    [Setup]    Setup Platform
    Send Trigger Pulse
    ${baseline}=    Count Echo High Reads
    Inject Distance    1600
    Send Trigger Pulse
    ${far}=    Count Echo High Reads
    Should Be True    ${far} > ${baseline}
    ...    msg=Echo HIGH count did not increase with distance: baseline=${baseline} far=${far}
    [Teardown]    Reset Emulation

2.7.2 Empty Tank — Maximum Distance Boundary (4000 mm)
    [Documentation]    Inject 4000 mm — within HC-SR04's 4 m max range.
    ...                pulse_reads(4000) ≈ 233. Echo HIGH must persist for
    ...                ≥100 reads. §2.7.2
    [Setup]    Setup Platform
    Inject Distance    4000
    Send Trigger Pulse
    ${high_count}=    Count Echo High Reads
    Should Be True    ${high_count} >= 100
    ...    msg=Expected echo HIGH ≥100 reads at 4000 mm, got ${high_count}
    [Teardown]    Reset Emulation

2.7.3 Full Tank — Minimum Distance Boundary (50 mm)
    [Documentation]    Inject 50 mm. pulse_reads clamps to a floor of 2, which
    ...                yields exactly 1 observable HIGH read (the second
    ...                decrement transitions state→idle and returns LOW), so
    ...                the echo window must be ≥1 but well below the default
    ...                800 mm baseline (~46 HIGH reads). §2.7.3
    [Setup]    Setup Platform
    Inject Distance    50
    Send Trigger Pulse
    ${high_count}=    Count Echo High Reads
    Should Be True    ${high_count} >= 1
    ...    msg=Echo HIGH count below pulse_reads floor at 50 mm: ${high_count}
    Should Be True    ${high_count} <= 5
    ...    msg=Echo HIGH count too large at 50 mm (should be near floor): ${high_count}
    [Teardown]    Reset Emulation

2.7.4 No-Echo Timeout When Trigger Pulse Never Sent
    [Documentation]    Without a trigger pulse, GPIO_IN reads must stay LOW —
    ...                the stub must not raise a spurious echo. Models the
    ...                firmware's 30 ms wait_until timeout path. §2.7.4
    [Setup]    Setup Platform
    ${high_count}=    Count Echo High Reads
    Should Be Equal As Integers    ${high_count}    0
    ...    msg=Echo went HIGH without a trigger pulse: ${high_count} HIGH reads
    [Teardown]    Reset Emulation

2.7.4 Echo Returns To Idle After One Cycle
    [Documentation]    After the pulse, the echo must fall LOW and stay LOW on
    ...                subsequent reads — no oscillation without a fresh trigger.
    ...                Drains one full pulse, then reads 5 more times. §2.7.4
    [Setup]    Setup Platform
    Send Trigger Pulse
    Count Echo High Reads
    FOR    ${i}    IN RANGE    5
        ${raw}=    Execute Command    sysbus ReadDoubleWord ${GPIO_IN}
        ${v}=      Convert To Integer    ${raw.strip()}
        ${echo}=   Evaluate    ${v} & ${ECHO_BIT}
        Should Be Equal As Integers    ${echo}    0
        ...    msg=Echo unexpectedly HIGH after idle (read ${i})
    END
    [Teardown]    Reset Emulation

2.7.1 Distance Magic Register Round-Trips
    [Documentation]    Sanity check: distance written to +0xFF0 reads back
    ...                identically.  Default reads as 800 mm. §2.7.1
    [Setup]    Setup Platform
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${GPIO_DISTANCE_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    800
    ...    msg=Default distance is not 800 mm
    Execute Command    sysbus WriteDoubleWord ${GPIO_DISTANCE_MAGIC} 1234
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${GPIO_DISTANCE_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    1234
    ...    msg=Distance write/read mismatch
    [Teardown]    Reset Emulation

*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    logLevel 0

Inject Distance
    [Documentation]    Writes the desired distance (mm) to the stub's magic
    ...                register; the stub recomputes pulse_reads on the spot.
    [Arguments]    ${mm}
    Execute Command    sysbus WriteDoubleWord ${GPIO_DISTANCE_MAGIC} ${mm}

Send Trigger Pulse
    [Documentation]    Drives the HC-SR04 trigger sequence:
    ...                set GPIO6 HIGH (W1TS), then clear it (W1TC). The clear
    ...                arms the echo cycle inside the stub.
    Execute Command    sysbus WriteDoubleWord ${GPIO_OUT_W1TS} ${TRIGGER_BIT}
    Execute Command    sysbus WriteDoubleWord ${GPIO_OUT_W1TC} ${TRIGGER_BIT}

Count Echo High Reads
    [Documentation]    Polls GPIO_IN bit 7 until LOW or until MAX_HIGH_READS
    ...                iterations have elapsed. Mirrors the firmware's
    ...                wait_while(HIGH) loop. Returns the number of HIGH reads
    ...                observed (= pulse_reads if a pulse was armed, 0 if not).
    ...                The first stub read is consumed by the 'pre' state, so
    ...                we discard it before counting.
    ${count}=    Set Variable    ${0}
    # Consume the 'pre'-state LOW read.
    Execute Command    sysbus ReadDoubleWord ${GPIO_IN}
    FOR    ${i}    IN RANGE    ${MAX_HIGH_READS}
        ${raw}=    Execute Command    sysbus ReadDoubleWord ${GPIO_IN}
        ${v}=      Convert To Integer    ${raw.strip()}
        ${echo}=   Evaluate    ${v} & ${ECHO_BIT}
        IF    ${echo} == 0
            BREAK
        END
        ${count}=    Evaluate    ${count} + 1
    END
    RETURN    ${count}
