*** Settings ***
Documentation     MCP73831T season-control register tests — exercises the season
...               register (+0xFFC) of mcp73831t.py directly via memory-mapped
...               reads/writes, without running firmware.
...
...               Season modes:
...                 0 = AUTO     — alternating day/night (50-tick phases)
...                 1 = SUNNY    — always daytime; solar charges at full rate
...                 2 = CLOUDY   — solar absent; MCP stays SHUTDOWN; battery drains
...                 3 = CRITICAL — force battery to 3100 mV immediately + drain
...
...               Simulates the outdoor Gagan OHT lifecycle:
...                 morning sun → battery charges (SUNNY)
...                 night / cloudy season → battery drains toward BatteryLow (CLOUDY)
...                 critical level → BatteryLow fires on next measurement (CRITICAL)
...                 recovery → sun returns, battery recharges (SUNNY after CLOUDY)
...
...               Tags: battery, mcp73831t, season, peripheral, outdoor

Resource          ${RENODEKEYWORDS}
Resource          ${CURDIR}${/}..${/}..${/}..${/}shared${/}robot-keywords${/}battery_keywords.robot

*** Variables ***
${PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}       ${CURDIR}${/}test_mcp73831t_platform.resc

*** Test Cases ***

# ── §S.1  Season register ──────────────────────────────────────────────────

S.1 Season Register Defaults To AUTO On Platform Load
    [Documentation]    Season register must read 0 (AUTO) immediately after platform
    ...                load — no prior state carried over.
    [Setup]    Setup Platform
    ${s}=    Read Season
    Should Be Equal As Integers    ${s}    0    msg=Default season not AUTO(0)
    [Teardown]    Reset Emulation

S.1 Season Register Write/Read Roundtrip
    [Documentation]    Each season code must read back the written value.
    [Setup]    Setup Platform
    Set Battery Season    1
    ${s}=    Read Season
    Should Be Equal As Integers    ${s}    1    msg=Season not SUNNY(1) after write
    Set Battery Season    2
    ${s}=    Read Season
    Should Be Equal As Integers    ${s}    2    msg=Season not CLOUDY(2) after write
    Set Battery Season    3
    ${s}=    Read Season
    Should Be Equal As Integers    ${s}    3    msg=Season not CRITICAL(3) after write
    Set Battery Season    0
    ${s}=    Read Season
    Should Be Equal As Integers    ${s}    0    msg=Season not AUTO(0) after restore
    [Teardown]    Reset Emulation

S.1 Season Register Resets After Platform Reload
    [Documentation]    Season must return to AUTO after a Reset/reload cycle.
    [Setup]    Setup Platform
    Set Battery Season    2
    Reset Emulation
    Setup Platform
    ${s}=    Read Season
    Should Be Equal As Integers    ${s}    0    msg=Season not AUTO after reload
    [Teardown]    Reset Emulation

# ── §S.2  Sunny season — battery charges ──────────────────────────────────

S.2 Sunny Season Charges A Depleted Battery
    [Documentation]    Start at 3200 mV with solar pre-warmed above VIN_MIN (3750 mV).
    ...                After 30 ticks the battery must have increased — confirms
    ...                FAST_CHARGE rate of 10 mV/tick is active.
    [Setup]    Setup Platform
    Set Battery Voltage    3200
    Set Solar Voltage      5000
    Set Season Sunny
    ${before}=    Read Battery Mv
    Advance Battery Simulation    30
    ${after}=    Read Battery Mv
    Should Be True    ${after} > ${before}
    ...    msg=Battery did not charge in SUNNY season: before=${before} mV after=${after} mV
    [Teardown]    Reset Emulation

S.2 Sunny Season Reaches Near-Full Charge
    [Documentation]    Starting at 3700 mV with solar pre-warmed, 60 ticks at
    ...                10 mV/tick must bring the battery above 4000 mV.
    [Setup]    Setup Platform
    Set Battery Voltage    3700
    Set Solar Voltage      5000
    Set Season Sunny
    Advance Battery Simulation    60
    Battery Voltage Should Be Above    4000
    [Teardown]    Reset Emulation

S.2 Sunny Season Drives CHG_STAT_N Low While Charging
    [Documentation]    During FAST_CHARGE the MCP73831T drives its STAT pin LOW.
    ...                CHG_STAT_N must read 0 after the first sunny tick.
    [Setup]    Setup Platform
    Set Battery Voltage    3700
    Set Solar Voltage      5000
    Set Season Sunny
    Advance Battery Simulation    1
    Solar Should Be Charging
    [Teardown]    Reset Emulation

S.2 Sunny Season Does Not Overcharge Past VREG
    [Documentation]    VREG cap (4200 mV): battery must not exceed the regulation
    ...                voltage even after 200 ticks.
    [Setup]    Setup Platform
    Set Battery Voltage    4100
    Set Solar Voltage      5000
    Set Season Sunny
    Advance Battery Simulation    200
    Battery Voltage Should Be Below    4201
    [Teardown]    Reset Emulation

# ── §S.3  Cloudy season — battery drains ──────────────────────────────────

S.3 Cloudy Season Drains Battery Below BatteryLow Threshold
    [Documentation]    Start at 3700 mV, switch to CLOUDY. 70 ticks × 5 mV/tick =
    ...                350 mV drain → lands below 3400 mV (the firmware BatteryLow
    ...                threshold: 20% SOC on Li-Ion). Confirms the scenario that
    ...                triggers OhtEvent::BatteryLow in firmware.
    [Setup]    Setup Platform
    Set Battery Voltage    3700
    Set Season Cloudy
    Advance Battery Simulation    70
    Battery Should Be In Low State
    [Teardown]    Reset Emulation

S.3 Cloudy Season Holds Solar At Zero
    [Documentation]    Even if solar_mv was pre-injected at 5000 mV, switching to
    ...                CLOUDY must force solar_mv to 0 on the next tick.
    [Setup]    Setup Platform
    Set Solar Voltage    5000
    Set Season Cloudy
    Advance Battery Simulation    1
    ${sol}=    Read Solar Mv
    Should Be Equal As Integers    ${sol}    0    msg=Solar not zero in CLOUDY season
    [Teardown]    Reset Emulation

S.3 Cloudy Season Charger Is Inactive
    [Documentation]    With VIN = 0 < VIN_MIN, MCP73831T enters SHUTDOWN →
    ...                CHG_STAT_N must be HIGH (not charging).
    [Setup]    Setup Platform
    Set Season Cloudy
    Advance Battery Simulation    1
    Solar Should Not Be Charging
    [Teardown]    Reset Emulation

S.3 Cloudy Season Drain Floor Is 3000 mV
    [Documentation]    The cloudy drain floor is 3000 mV; starting at 3100 mV and
    ...                running 50 ticks must not push the battery below 3000 mV.
    [Setup]    Setup Platform
    Set Battery Voltage    3100
    Set Season Cloudy
    Advance Battery Simulation    50
    ${v}=    Read Battery Mv
    Should Be True    ${v} >= 3000
    ...    msg=Battery drained below cloudy floor: ${v} mV (expected ≥ 3000 mV)
    [Teardown]    Reset Emulation

# ── §S.4  Critical season — immediate low battery ─────────────────────────

S.4 Critical Season Forces Battery To 3100 mV On Write
    [Documentation]    Writing season=CRITICAL (3) must immediately set battery_mv
    ...                to 3100 mV without any ticks. 3100 mV is below the BatteryLow
    ...                threshold (~3400 mV / 20% SOC), so the next firmware
    ...                measurement will post OhtEvent::BatteryLow.
    [Setup]    Setup Platform
    Set Battery Voltage    4200
    Force Battery Critical
    ${v}=    Read Battery Mv
    Should Be Equal As Integers    ${v}    3100
    ...    msg=CRITICAL season did not force battery_mv to 3100 mV
    [Teardown]    Reset Emulation

S.4 Critical Season Clears Solar On Write
    [Documentation]    Writing season=CRITICAL also zeros solar_mv immediately.
    [Setup]    Setup Platform
    Set Solar Voltage    5000
    Force Battery Critical
    ${v}=    Read Solar Mv
    Should Be Equal As Integers    ${v}    0
    ...    msg=Solar not zeroed by CRITICAL season write
    [Teardown]    Reset Emulation

S.4 Critical Season Continues Draining On Subsequent Ticks
    [Documentation]    After the instant 3100 mV injection, ticks continue to
    ...                drain the battery (same as CLOUDY). 20 ticks × 5 mV/tick
    ...                must reduce battery below 3100 mV.
    [Setup]    Setup Platform
    Force Battery Critical
    Advance Battery Simulation    20
    ${v}=    Read Battery Mv
    Should Be True    ${v} < 3100
    ...    msg=Battery did not drain below 3100 mV after CRITICAL ticks: ${v} mV
    [Teardown]    Reset Emulation

# ── §S.5  Recovery — sunny after cloudy ───────────────────────────────────

S.5 Sunny Season After Cloudy Recharges Battery
    [Documentation]    Full outdoor cycle: drain to near-critical with CLOUDY,
    ...                then switch to SUNNY (morning sun). Battery must recover
    ...                upward from the drained state within 20 ticks.
    [Setup]    Setup Platform
    Set Battery Voltage    3700
    Set Season Cloudy
    Advance Battery Simulation    70
    Battery Should Be In Low State
    ${drained}=    Read Battery Mv
    # Pre-warm solar and switch to sunny (morning sun arrives)
    Set Solar Voltage    5000
    Set Season Sunny
    Advance Battery Simulation    20
    ${recovered}=    Read Battery Mv
    Should Be True    ${recovered} > ${drained}
    ...    msg=Battery did not recover: drained=${drained} mV recovered=${recovered} mV
    [Teardown]    Reset Emulation

S.5 Auto Season Restores Day/Night Cycle After Cloudy
    [Documentation]    Reverting from CLOUDY back to AUTO must leave the season
    ...                register at 0 and resume the tick-based day/night cycle.
    [Setup]    Setup Platform
    Set Season Cloudy
    Advance Battery Simulation    10
    Set Season Auto
    ${s}=    Read Season
    Should Be Equal As Integers    ${s}    0    msg=Season not AUTO after restore
    [Teardown]    Reset Emulation

*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    logLevel 0
