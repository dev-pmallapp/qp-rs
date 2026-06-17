*** Settings ***
Documentation     Shared keywords for the MCP73831T battery/solar simulation on the
...               Gagan node (ESP32-C6, ADC base 0x6000E000).
...
...               Provides season control, direct voltage injection, ADC tick triggers,
...               and assertion helpers for both peripheral-level and integration tests.
...
...               Magic register map:
...                 0x6000EFF0  r/w  battery_mv
...                 0x6000EFF4  r/w  solar_mv
...                 0x6000EFF8  r/w  chg_stat_n  (0=charging/LOW, 1=idle/Hi-Z)
...                 0x6000EFFC  r/w  season      (0=AUTO, 1=SUNNY, 2=CLOUDY, 3=CRITICAL)
...
...               ADC registers (reading triggers one simulation tick):
...                 0x6000E040  SAR1DATA_STATUS  — battery ADC count
...                 0x6000E044  SAR2DATA_STATUS  — solar ADC count
Resource          ${RENODEKEYWORDS}

*** Variables ***
${MCP_SAR1}              0x6000E040
${MCP_SAR2}              0x6000E044
${MCP_BAT_REG}           0x6000EFF0
${MCP_SOL_REG}           0x6000EFF4
${MCP_CHG_REG}           0x6000EFF8
${MCP_SEASON_REG}        0x6000EFFC

${SEASON_AUTO}           0
${SEASON_SUNNY}          1
${SEASON_CLOUDY}         2
${SEASON_CRITICAL}       3

# Firmware BatteryLow threshold: low_threshold_percent=20 in BatteryManager.
# From Li-Ion LUT (battery.rs): 20% SOC ≈ 3400 mV.
${BATTERY_LOW_MV}        3400

*** Keywords ***

# ── Season control ────────────────────────────────────────────────────────────

Set Battery Season
    [Documentation]    Set MCP73831T simulation season via magic register.
    ...                0=AUTO (day/night cycle)  1=SUNNY  2=CLOUDY  3=CRITICAL
    [Arguments]        ${season}
    Execute Command    sysbus WriteDoubleWord ${MCP_SEASON_REG} ${season}

Set Season Sunny
    [Documentation]    Force always-daytime: solar rises to 5 V and MCP73831T
    ...                enters FAST_CHARGE. Battery charges at ~10 mV/tick.
    ...                Pre-warm solar_mv above VIN_MIN (3750 mV) via Set Solar Voltage
    ...                for immediate effect; otherwise charging starts after ~38 ticks.
    Set Battery Season    ${SEASON_SUNNY}

Set Season Cloudy
    [Documentation]    Force overcast season: solar is held at 0 mV, MCP73831T
    ...                stays in SHUTDOWN, battery drains at 5 mV/tick down to
    ...                the 3000 mV floor (well below the BatteryLow threshold).
    Set Battery Season    ${SEASON_CLOUDY}

Set Season Auto
    [Documentation]    Restore the default alternating day/night cycle
    ...                (50-tick phases, matching the original mcp73831t.py behaviour).
    Set Battery Season    ${SEASON_AUTO}

Force Battery Critical
    [Documentation]    Write CRITICAL season: instantly forces battery_mv to 3100 mV
    ...                and solar_mv to 0. On the next firmware measurement cycle
    ...                BatteryManager will see ~2% SOC (< 20% threshold) and post
    ...                OhtEvent::BatteryLow → AppCoordinatorAO enters Fault.
    Set Battery Season    ${SEASON_CRITICAL}

# ── Direct voltage injection ──────────────────────────────────────────────────

Set Battery Voltage
    [Documentation]    Directly set battery_mv. Does not trigger a simulation tick.
    [Arguments]        ${mv}
    Execute Command    sysbus WriteDoubleWord ${MCP_BAT_REG} ${mv}

Set Solar Voltage
    [Documentation]    Directly set solar_mv. Use to pre-warm solar above VIN_MIN
    ...                (3750 mV) before switching to SUNNY season for immediate
    ...                charging without waiting ~38 warm-up ticks.
    [Arguments]        ${mv}
    Execute Command    sysbus WriteDoubleWord ${MCP_SOL_REG} ${mv}

Set Charger Status
    [Documentation]    Override CHG_STAT_N directly (0=charging, 1=not charging).
    ...                Effect lasts until the next ADC read tick recomputes it.
    [Arguments]        ${chg_stat_n}
    Execute Command    sysbus WriteDoubleWord ${MCP_CHG_REG} ${chg_stat_n}

# ── ADC tick trigger ──────────────────────────────────────────────────────────

Advance Battery Simulation
    [Documentation]    Trigger ${count} ADC read ticks in the MCP73831T model by
    ...                reading SAR1DATA_STATUS. Each read advances the season
    ...                simulation by one step (charge or drain depending on season).
    [Arguments]        ${count}
    FOR    ${i}    IN RANGE    0    ${count}
        Execute Command    sysbus ReadDoubleWord ${MCP_SAR1}
    END

# ── Magic-register reads (no tick) ───────────────────────────────────────────

Read Battery Mv
    [Documentation]    Read battery_mv from the magic register. Does not trigger a tick.
    ${raw}=            Execute Command    sysbus ReadDoubleWord ${MCP_BAT_REG}
    ${v}=              Convert To Integer    ${raw.strip()}
    RETURN             ${v}

Read Solar Mv
    [Documentation]    Read solar_mv from the magic register. Does not trigger a tick.
    ${raw}=            Execute Command    sysbus ReadDoubleWord ${MCP_SOL_REG}
    ${v}=              Convert To Integer    ${raw.strip()}
    RETURN             ${v}

Read Charger Status
    [Documentation]    Read chg_stat_n (0=charging/STAT-LOW, 1=idle/STAT-Hi-Z).
    ${raw}=            Execute Command    sysbus ReadDoubleWord ${MCP_CHG_REG}
    ${v}=              Convert To Integer    ${raw.strip()}
    RETURN             ${v}

Read Season
    [Documentation]    Read back the current season register value.
    ${raw}=            Execute Command    sysbus ReadDoubleWord ${MCP_SEASON_REG}
    ${v}=              Convert To Integer    ${raw.strip()}
    RETURN             ${v}

# ── Assertions ────────────────────────────────────────────────────────────────

Battery Voltage Should Be Above
    [Arguments]        ${threshold_mv}
    ${v}=              Read Battery Mv
    Should Be True    ${v} > ${threshold_mv}
    ...    msg=Battery ${v} mV is not above ${threshold_mv} mV

Battery Voltage Should Be Below
    [Arguments]        ${threshold_mv}
    ${v}=              Read Battery Mv
    Should Be True    ${v} < ${threshold_mv}
    ...    msg=Battery ${v} mV is not below ${threshold_mv} mV

Battery Should Be In Low State
    [Documentation]    Assert battery is below the firmware BatteryLow threshold
    ...                (3400 mV / 20% SOC on Li-Ion). The next firmware measurement
    ...                cycle will post OhtEvent::BatteryLow.
    Battery Voltage Should Be Below    ${BATTERY_LOW_MV}

Solar Should Be Charging
    [Documentation]    Assert CHG_STAT_N is LOW — MCP73831T is actively charging
    ...                (PRECHARGE or FAST_CHARGE state).
    ${v}=              Read Charger Status
    Should Be Equal As Integers    ${v}    0
    ...    msg=Charger not active (CHG_STAT_N=${v}, expected 0)

Solar Should Not Be Charging
    [Documentation]    Assert CHG_STAT_N is HIGH — MCP73831T is in SHUTDOWN or
    ...                COMPLETE (no active charge current).
    ${v}=              Read Charger Status
    Should Be Equal As Integers    ${v}    1
    ...    msg=Charger unexpectedly active (CHG_STAT_N=${v}, expected 1)
