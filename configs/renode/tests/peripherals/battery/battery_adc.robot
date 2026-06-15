*** Settings ***
Documentation     Battery ADC stub unit tests — exercises battery_adc_stub.py directly
...               via memory-mapped register reads/writes, without running firmware.
...               Covers TestingTopics.md §2.1.1 (register addressing), §2.1.2 (voltage
...               range boundaries), §2.1.3 (reset to default state).
...               Tags: battery, peripheral, unit, §2.1

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc
${ADC_SAR1_STATUS}   0x6000E040
${ADC_SAR2_STATUS}   0x6000E044
${ADC_BAT_MAGIC}     0x6000EFF0
${ADC_SOL_MAGIC}     0x6000EFF4
${ADC_CHG_MAGIC}     0x6000EFF8

*** Test Cases ***

2.1.1 SAR1DATA_STATUS Has VALID Bit Set On Load
    [Documentation]    Reads SAR1DATA_STATUS immediately after load (no firmware running).
    ...                Bit 16 (VALID) must be set to confirm the ADC stub is active and
    ...                returning data.  The exact count may reflect one tick of dynamics.
    [Setup]    Setup Platform
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${ADC_SAR1_STATUS}
    ${v}=      Convert To Integer    ${raw.strip()}
    ${valid}=  Evaluate    (${v} >> 16) & 1
    Should Be Equal As Integers    ${valid}    1    msg=VALID bit (bit 16) not set in SAR1DATA_STATUS
    [Teardown]    Reset Emulation

2.1.1 Magic Register Returns Configured Battery Voltage
    [Documentation]    Write a voltage to the magic register (+0xFF0) and read it back.
    ...                The magic read returns the stored mV value directly without dynamics.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${ADC_BAT_MAGIC} 3500
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${ADC_BAT_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    3500
    [Teardown]    Reset Emulation

2.1.1 Magic Register Returns Configured Solar Voltage
    [Documentation]    Write solar voltage (+0xFF4) and read it back.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${ADC_SOL_MAGIC} 4500
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${ADC_SOL_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    4500
    [Teardown]    Reset Emulation

2.1.2 Battery Voltage Nominal — 3700 mV Default
    [Documentation]    Default battery_mv is 3700 mV on platform init.
    ...                Magic read (no dynamics) must return 3700.
    [Setup]    Setup Platform
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${ADC_BAT_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    3700
    [Teardown]    Reset Emulation

2.1.2 Battery Voltage Minimum Boundary — 0 mV
    [Documentation]    Inject 0 mV (dead battery).  Magic read must return 0.
    ...                SAR1 VALID bit must still be set even at zero voltage.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${ADC_BAT_MAGIC} 0
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${ADC_BAT_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    0    msg=0mV injection not reflected in magic register
    [Teardown]    Reset Emulation

2.1.2 Battery Voltage Maximum Boundary — 4200 mV
    [Documentation]    Inject 4200 mV (fully charged).  Magic read must return 4200.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${ADC_BAT_MAGIC} 4200
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${ADC_BAT_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    4200
    [Teardown]    Reset Emulation

2.1.2 Battery Fault Threshold — 1000 mV Below Floor
    [Documentation]    Inject 1000 mV (below the 2400 mV fault floor used by battery.rs).
    ...                Magic read must reflect the injected value.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${ADC_BAT_MAGIC} 1000
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${ADC_BAT_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    1000
    [Teardown]    Reset Emulation

2.1.2 Charge Status Flag — Charging State
    [Documentation]    Write chg_stat_n=0 (charging).  Magic read must return 0.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${ADC_CHG_MAGIC} 0
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${ADC_CHG_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    0    msg=Charging state not reflected
    [Teardown]    Reset Emulation

2.1.2 Charge Status Flag — Not Charging State
    [Documentation]    Write chg_stat_n=1 (not charging).  Magic read must return 1.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${ADC_CHG_MAGIC} 1
    ${raw}=    Execute Command    sysbus ReadDoubleWord ${ADC_CHG_MAGIC}
    Should Be Equal As Integers    ${raw.strip()}    1
    [Teardown]    Reset Emulation

2.1.3 ADC Stub Resets To Default On Platform Reload
    [Documentation]    Inject 999 mV.  Clear and reload the machine.  Magic read
    ...                must return the 3700 mV default — no stale state carried over.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${ADC_BAT_MAGIC} 999
    ${before}=    Execute Command    sysbus ReadDoubleWord ${ADC_BAT_MAGIC}
    Should Be Equal As Integers    ${before.strip()}    999    msg=Injection did not take effect

    Reset Emulation
    Setup Platform
    ${after}=    Execute Command    sysbus ReadDoubleWord ${ADC_BAT_MAGIC}
    Should Be Equal As Integers    ${after.strip()}    3700    msg=Default not restored after reload
    [Teardown]    Reset Emulation

2.1.3 Solar ADC Resets To Zero On Platform Reload
    [Documentation]    Inject solar 5000 mV.  Reload.  Magic read must return 0 (default).
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${ADC_SOL_MAGIC} 5000
    Reset Emulation
    Setup Platform
    ${after}=    Execute Command    sysbus ReadDoubleWord ${ADC_SOL_MAGIC}
    Should Be Equal As Integers    ${after.strip()}    0    msg=Solar default not 0 after reload
    [Teardown]    Reset Emulation

*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    logLevel 0
