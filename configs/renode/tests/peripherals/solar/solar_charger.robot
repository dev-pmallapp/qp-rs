*** Settings ***
Documentation     Solar charger stub unit tests — exercises
...               solar_charger_stub.py through direct register reads/writes,
...               without running firmware.
...
...               Stub base: 0x60010000 (test-only — production firmware reads
...               the same semantics through SAR2 of battery_adc_stub.py).
...
...               Validates the solar-charging state-derivation rules:
...                 solar_present   = (solar_mv > 3500) and not night
...                 charge_source   = Solar | BatteryAndSolar | Battery | None
...                 charging_active = solar_present and not battery_full
...
...               Covers TestingTopics.md §2.8.1 (solar voltage ADC → solar_present),
...               §2.8.2 (charging-status flag / ChargeSource enum bit), §2.8.3
...               (night clears the flag), §2.8.4 (battery-full → float mode).
...               Tags: solar, charger, peripheral, unit, §2.8

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${CURDIR}${/}test_solar_charger_platform.resc

${SOLAR_BASE}             0x60010000
${REG_SOLAR_MV}           0x60010000
${REG_CHARGE_SOURCE}      0x60010004
${REG_CHARGING_ACTIVE}    0x60010008
${REG_SOLAR_PRESENT}      0x6001000C
${REG_SET_SOLAR_MV}       0x60010FF0
${REG_SET_NIGHT}          0x60010FF4
${REG_SET_BATTERY_FULL}   0x60010FF8

# ChargeSource enum codes (mirrors swm-core::power::ChargeSource).
${CS_NONE}                0
${CS_BATTERY}             1
${CS_SOLAR}               2
${CS_BATTERY_AND_SOLAR}   3

*** Test Cases ***

2.8.1 Default State Has No Solar Voltage And ChargeSource None
    [Documentation]    On platform load the stub reports 0 mV, no solar
    ...                presence, no charging activity, and ChargeSource::None.
    ...                §2.8.1
    [Setup]    Setup Platform
    Solar Voltage Should Be    0
    Solar Present Should Be    0
    Charging Active Should Be    0
    Charge Source Should Be    ${CS_NONE}
    [Teardown]    Reset Emulation

2.8.1 Voltage Above Threshold Sets solar_present
    [Documentation]    SOLAR_MV > 3500 mV must set solar_present and select
    ...                ChargeSource::Solar (the default-day path). §2.8.1
    [Setup]    Setup Platform
    Set Solar Voltage    4000
    Solar Voltage Should Be    4000
    Solar Present Should Be    1
    Charging Active Should Be    1
    Charge Source Should Be    ${CS_SOLAR}
    [Teardown]    Reset Emulation

2.8.1 Voltage At Threshold (3500 mV) Does Not Trip solar_present
    [Documentation]    The rule is strictly > 3500 mV, so the boundary value
    ...                must clear the flag and ChargeSource falls to Battery
    ...                (non-zero mV but below threshold). §2.8.1
    [Setup]    Setup Platform
    Set Solar Voltage    3500
    Solar Present Should Be    0
    Charge Source Should Be    ${CS_BATTERY}
    [Teardown]    Reset Emulation

2.8.1 Just Above Threshold (3501 mV) Trips solar_present
    [Documentation]    One millivolt above the boundary is enough — confirms
    ...                strict-greater-than semantics. §2.8.1
    [Setup]    Setup Platform
    Set Solar Voltage    3501
    Solar Present Should Be    1
    Charge Source Should Be    ${CS_SOLAR}
    [Teardown]    Reset Emulation

2.8.2 ChargeSource Bit Transitions With Voltage
    [Documentation]    Walks the enum: None → Battery → Solar by raising the
    ...                voltage. §2.8.2
    [Setup]    Setup Platform
    Charge Source Should Be    ${CS_NONE}
    Set Solar Voltage    2000
    Charge Source Should Be    ${CS_BATTERY}
    Set Solar Voltage    4200
    Charge Source Should Be    ${CS_SOLAR}
    [Teardown]    Reset Emulation

2.8.3 Night Flag Forces solar_present Clear Even With Sun Voltage
    [Documentation]    Set 4500 mV (would normally trip solar_present), then
    ...                set NIGHT = 1. solar_present must clear and the source
    ...                must fall back to Battery (non-zero mV). §2.8.3
    [Setup]    Setup Platform
    Set Solar Voltage    4500
    Solar Present Should Be    1
    Set Night    1
    Solar Present Should Be    0
    Charging Active Should Be    0
    Charge Source Should Be    ${CS_BATTERY}
    [Teardown]    Reset Emulation

2.8.3 Night-Then-Day Restores solar_present
    [Documentation]    Once NIGHT clears, solar_present returns based on the
    ...                voltage that is still configured. §2.8.3
    [Setup]    Setup Platform
    Set Solar Voltage    4500
    Set Night    1
    Solar Present Should Be    0
    Set Night    0
    Solar Present Should Be    1
    Charge Source Should Be    ${CS_SOLAR}
    [Teardown]    Reset Emulation

2.8.4 Battery-Full Float Mode Reports BatteryAndSolar
    [Documentation]    With sun voltage AND battery_full set, the charger
    ...                model must report ChargeSource::BatteryAndSolar (float
    ...                CC/CV mode) and clear charging_active. §2.8.4
    [Setup]    Setup Platform
    Set Solar Voltage    4500
    Charging Active Should Be    1
    Charge Source Should Be    ${CS_SOLAR}
    Set Battery Full    1
    Charging Active Should Be    0
    Charge Source Should Be    ${CS_BATTERY_AND_SOLAR}
    Solar Present Should Be    1
    [Teardown]    Reset Emulation

2.8.4 Battery-Full Clears Returns To Solar Charging
    [Documentation]    Once battery_full clears, charging_active goes back to
    ...                1 and ChargeSource falls back to Solar. §2.8.4
    [Setup]    Setup Platform
    Set Solar Voltage    4500
    Set Battery Full    1
    Charge Source Should Be    ${CS_BATTERY_AND_SOLAR}
    Set Battery Full    0
    Charge Source Should Be    ${CS_SOLAR}
    Charging Active Should Be    1
    [Teardown]    Reset Emulation

2.8.1 Solar Voltage Persists And Reads Back Through Magic Register
    [Documentation]    The +0xFF0 magic register must echo the written value;
    ...                the +0x000 sense register must report the same value
    ...                (the model has no ADC dynamics — deterministic). §2.8.1
    [Setup]    Setup Platform
    Set Solar Voltage    3700
    ${magic}=    Read Register    ${REG_SET_SOLAR_MV}
    Should Be Equal As Integers    ${magic}    3700
    ${sense}=    Read Register    ${REG_SOLAR_MV}
    Should Be Equal As Integers    ${sense}    3700
    [Teardown]    Reset Emulation

2.8.1 Stub Resets To Default On Platform Reload
    [Documentation]    Inject 4500 mV and night=1. Reset the emulation, reload
    ...                — solar_mv must be 0 and night must be 0. §2.8.1
    [Setup]    Setup Platform
    Set Solar Voltage    4500
    Set Night    1
    Reset Emulation
    Setup Platform
    Solar Voltage Should Be    0
    ${night}=    Read Register    ${REG_SET_NIGHT}
    Should Be Equal As Integers    ${night}    0
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

Set Solar Voltage
    [Arguments]    ${mv}
    Execute Command    sysbus WriteDoubleWord ${REG_SET_SOLAR_MV} ${mv}

Set Night
    [Arguments]    ${flag}
    Execute Command    sysbus WriteDoubleWord ${REG_SET_NIGHT} ${flag}

Set Battery Full
    [Arguments]    ${flag}
    Execute Command    sysbus WriteDoubleWord ${REG_SET_BATTERY_FULL} ${flag}

Solar Voltage Should Be
    [Arguments]    ${expected}
    ${v}=    Read Register    ${REG_SOLAR_MV}
    Should Be Equal As Integers    ${v}    ${expected}    msg=Solar mV mismatch

Solar Present Should Be
    [Arguments]    ${expected}
    ${v}=    Read Register    ${REG_SOLAR_PRESENT}
    Should Be Equal As Integers    ${v}    ${expected}    msg=solar_present mismatch

Charging Active Should Be
    [Arguments]    ${expected}
    ${v}=    Read Register    ${REG_CHARGING_ACTIVE}
    Should Be Equal As Integers    ${v}    ${expected}    msg=charging_active mismatch

Charge Source Should Be
    [Arguments]    ${expected}
    ${v}=    Read Register    ${REG_CHARGE_SOURCE}
    Should Be Equal As Integers    ${v}    ${expected}    msg=charge_source mismatch
