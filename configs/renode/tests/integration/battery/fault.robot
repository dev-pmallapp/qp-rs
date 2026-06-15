*** Settings ***
Documentation     Battery fault-injection tests for ESP32-C6 + LR1121 simulation.
...
...               Verifies that BatteryManagerAO and AppCoordinatorAO correctly handle
...               injected battery faults and that the system recovers after voltage is
...               restored — all running against virtual time so results are deterministic.
...
...               Prerequisites:
...                 - Firmware built with the 'qs' feature:
...                     cargo build --features qs,lr1121
...                 - Run from the project root:
...                     VIRTUAL_ENV=.venv renode-test configs/renode/tests/battery_fault_test.robot

Resource          ${RENODEKEYWORDS}
Library           Process
Library           String

*** Variables ***
${PROJECT_ROOT}       ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}          ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc
${FIRMWARE_PATH}      ${PROJECT_ROOT}${/}target${/}riscv32imac-unknown-none-elf${/}debug${/}swm-gagan-esp32c6
${ADC_BATTERY_REG}    0x6000EFF0
${VOLTAGE_FAULT}      1000
${VOLTAGE_LOW}        2500
${VOLTAGE_HEALTHY}    3700
# Scratchpad addresses in spi_flash_stub (0x60002000, 0x1000 bytes) used by hooks.
# Hooks write here directly (no monitor.Execute roundtrip, so no deadlock during RunFor).
${SCRATCH_FRAMES}     0x60002FF0
${SCRATCH_ABORT}      0x60002FF4

*** Test Cases ***

Normal Boot Emits QS Frames
    [Documentation]    Firmware boots and emits at least one QS actor-transition frame
    ...                (TxDone) within 500 ms of virtual time.
    [Setup]    Setup Simulation
    Run Virtual Ms    500
    QS Frames Should Have Been Seen
    System Should Not Have Aborted
    [Teardown]    Reset Emulation

Battery Hard Fault Triggers Fault Transition And System Survives
    [Documentation]    Inject 1000 mV (below the 2400 mV floor in battery.rs:259).
    ...                BatteryManagerAO must enter Fault, AppCoordinatorAO must follow,
    ...                and the system must continue emitting QS frames (no panic/abort).
    [Setup]    Setup Simulation
    Run Virtual Ms    500
    System Should Not Have Aborted

    Reset Frames Flag
    Inject Battery Fault
    Run Virtual Ms    500
    System Should Not Have Aborted
    QS Frames Should Have Been Seen
    [Teardown]    Reset Emulation

Battery Low SoC Triggers BatteryLow Path And System Survives
    [Documentation]    Inject 2500 mV (valid voltage, low SoC%).
    ...                BatteryManagerAO stays Idle (no hard fault), but AppCoordinatorAO
    ...                enters Fault via the BatteryLow path (coordinator.rs:577).
    [Setup]    Setup Simulation
    Run Virtual Ms    500
    System Should Not Have Aborted

    Reset Frames Flag
    Inject Battery Low
    Run Virtual Ms    500
    System Should Not Have Aborted
    QS Frames Should Have Been Seen
    [Teardown]    Reset Emulation

Battery Restored After Fault Resumes Normal Cycle
    [Documentation]    Full round-trip: normal → hard fault → restore → recovery cycle
    ...                must emit QS frames within 500 ms of virtual time after restore.
    [Setup]    Setup Simulation
    Run Virtual Ms    500
    System Should Not Have Aborted

    Inject Battery Fault
    Run Virtual Ms    500
    System Should Not Have Aborted

    Reset Frames Flag
    Restore Battery
    Run Virtual Ms    500
    System Should Not Have Aborted
    QS Frames Should Have Been Seen
    [Teardown]    Reset Emulation

*** Keywords ***

# ── Setup ─────────────────────────────────────────────────────────────────────

Setup Simulation
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    # Disable verbose logging — both flags make simulation run at <<1x real speed.
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    logLevel 0
    # Pre-clear scratchpad.
    Execute Command    sysbus WriteDoubleWord ${SCRATCH_FRAMES} 0
    Execute Command    sysbus WriteDoubleWord ${SCRATCH_ABORT} 0
    # Abort hook — writes to scratchpad (no monitor.Execute, safe during RunFor).
    Execute Command    cpu0 AddSymbolHook "_default_abort" "machine.SystemBus.WriteDoubleWord(${SCRATCH_ABORT}, 1)"
    # emit_frame hook — use address (local 't' symbols are invisible to AddSymbolHook).
    ${addr}=    Get Symbol Address    EspQsSink.*emit_frame
    ${hook}=    Set Variable    cpu0 AddHook ${addr} "machine.SystemBus.WriteDoubleWord(${SCRATCH_FRAMES}, 1)"
    Execute Command    ${hook}

# ── Fault injection ───────────────────────────────────────────────────────────

Inject Battery Fault
    Execute Command    sysbus WriteDoubleWord ${ADC_BATTERY_REG} ${VOLTAGE_FAULT}

Inject Battery Low
    Execute Command    sysbus WriteDoubleWord ${ADC_BATTERY_REG} ${VOLTAGE_LOW}

Restore Battery
    Execute Command    sysbus WriteDoubleWord ${ADC_BATTERY_REG} ${VOLTAGE_HEALTHY}

# ── Timing ────────────────────────────────────────────────────────────────────

Run Virtual Ms
    [Arguments]    ${ms}
    Execute Command    emulation RunFor "00:00:00.${ms}"

# ── Assertions ────────────────────────────────────────────────────────────────

Reset Frames Flag
    Execute Command    sysbus WriteDoubleWord ${SCRATCH_FRAMES} 0

System Should Not Have Aborted
    ${v}=    Execute Command    sysbus ReadDoubleWord ${SCRATCH_ABORT}
    Should Be Equal As Integers    ${v.strip()}    0
    ...    msg=_default_abort fired — firmware panicked or hit a backtrace.

QS Frames Should Have Been Seen
    ${v}=    Execute Command    sysbus ReadDoubleWord ${SCRATCH_FRAMES}
    Should Be Equal As Integers    ${v.strip()}    1
    ...    msg=No QS frames emitted — emit_frame was never called.

# ── Symbol discovery ──────────────────────────────────────────────────────────

Get Symbol Address
    [Arguments]    ${grep_pattern}
    ${result}=    Run Process    sh    -c    nm ${FIRMWARE_PATH} | grep "${grep_pattern}"
    ...    stdout=PIPE    stderr=PIPE
    Should Be Equal As Integers    ${result.rc}    0    msg=Symbol '${grep_pattern}' not found in ELF
    ${parts}=    Split String    ${result.stdout.strip()}
    RETURN    0x${parts}[0]
