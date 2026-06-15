*** Settings ***
Resource    ${RENODEKEYWORDS}
Library     Process
Library     String

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc
${FIRMWARE_PATH}     ${PROJECT_ROOT}${/}target${/}riscv32imac-unknown-none-elf${/}debug${/}swm-gagan-esp32c6
# Scratchpad words in spi_flash_stub (0x60002000, size 0x1000).
${SCRATCH_FRAMES_SEEN}    0x60002FF0
${SCRATCH_ABORT_FIRED}    0x60002FF4

*** Keywords ***
Load Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    # Disable verbose logging — both flags make simulation run at <<1x real speed.
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    logLevel 0

Clear Machine
    Run Keyword And Ignore Error    Reset Emulation

Get Symbol Address
    [Arguments]    ${grep_pattern}
    ${result}=    Run Process    sh    -c    nm ${FIRMWARE_PATH} | grep "${grep_pattern}"
    ...    stdout=PIPE    stderr=PIPE
    Should Be Equal As Integers    ${result.rc}    0    msg=Symbol matching '${grep_pattern}' not found
    ${parts}=    Split String    ${result.stdout.strip()}
    RETURN    0x${parts}[0]

Symbol Exists In ELF
    [Arguments]    ${grep_pattern}
    ${result}=    Run Process    sh    -c    nm ${FIRMWARE_PATH} | grep "${grep_pattern}"
    ...    stdout=PIPE    stderr=PIPE
    RETURN    ${result.rc}

*** Test Cases ***
Renode Variables Read And Write
    [Setup]    Load Platform
    Execute Command    set test_flag 0
    ${v}=    Execute Command    echo $test_flag
    Should Be Equal As Integers    ${v.strip()}    0
    Execute Command    set test_flag 42
    ${v}=    Execute Command    echo $test_flag
    Should Be Equal As Integers    ${v.strip()}    42
    [Teardown]    Clear Machine

emit_frame Address Found In ELF
    ${rc}=    Symbol Exists In ELF    EspQsSink.*emit_frame
    Skip If    ${rc} != 0    EspQsSink.emit_frame absent — build with ESP_FEATURES=qs
    ${addr}=    Get Symbol Address    EspQsSink.*emit_frame
    Should Not Be Empty    ${addr}
    Log    emit_frame @ ${addr}

Scratchpad Memory Is Writable
    [Setup]    Load Platform
    Execute Command    sysbus WriteDoubleWord ${SCRATCH_FRAMES_SEEN} 0
    ${v}=    Execute Command    sysbus ReadDoubleWord ${SCRATCH_FRAMES_SEEN}
    Should Be Equal As Integers    ${v.strip()}    0
    Execute Command    sysbus WriteDoubleWord ${SCRATCH_FRAMES_SEEN} 0xBEEF
    ${v}=    Execute Command    sysbus ReadDoubleWord ${SCRATCH_FRAMES_SEEN}
    Should Be Equal As Integers    ${v.strip()}    0xBEEF
    [Teardown]    Clear Machine

RunFor Triggers emit_frame Via Address Hook
    [Setup]    Load Platform
    ${rc}=    Symbol Exists In ELF    EspQsSink.*emit_frame
    Skip If    ${rc} != 0    EspQsSink.emit_frame absent — build with ESP_FEATURES=qs
    Execute Command    sysbus WriteDoubleWord ${SCRATCH_FRAMES_SEEN} 0
    # Use cpu0 AddHook <address> — works for local (t) symbols; AddSymbolHook only works for global (T).
    ${addr}=    Get Symbol Address    EspQsSink.*emit_frame
    ${hook}=    Set Variable    cpu0 AddHook ${addr} "machine.SystemBus.WriteDoubleWord(${SCRATCH_FRAMES_SEEN}, 1)"
    Execute Command    ${hook}
    Execute Command    emulation RunFor "00:00:03"
    ${v}=    Execute Command    sysbus ReadDoubleWord ${SCRATCH_FRAMES_SEEN}
    Should Be Equal As Integers    ${v.strip()}    1
    ...    msg=emit_frame hook never fired in 3 virtual seconds.
    [Teardown]    Clear Machine
