*** Settings ***
Documentation     Shared Robot Framework keywords usable by all platform test suites.
...               Import with:  Resource  ${CURDIR}/../../shared/robot-keywords/common_keywords.robot
Library           Collections
Resource          ${RENODEKEYWORDS}

*** Variables ***
${DEFAULT_UART_TIMEOUT}    15
${DEFAULT_GDB_PORT}        3333

*** Keywords ***

# ── Machine lifecycle ─────────────────────────────────────────────

Create Platform
    [Documentation]    Create a named Renode machine and load a platform description.
    [Arguments]        ${machine_name}    ${repl_path}
    Execute Command    mach create "${machine_name}"
    Execute Command    machine LoadPlatformDescription @${repl_path}

Load Firmware ELF
    [Documentation]    Load an ELF file into the current machine's sysbus.
    [Arguments]        ${elf_path}
    Execute Command    sysbus LoadELF @${elf_path}

Load Firmware Binary
    [Documentation]    Load a raw binary at a specific address.
    [Arguments]        ${bin_path}    ${load_address}
    Execute Command    sysbus LoadBinary @${bin_path} ${load_address}

Start And Wait
    [Documentation]    Start execution and wait a short time for the CPU to run.
    [Arguments]        ${delay_s}=0.5
    Execute Command    start
    Sleep              ${delay_s}s

Reset Machine
    Execute Command    pause
    Execute Command    machine Reset

# ── UART helpers ──────────────────────────────────────────────────

Open UART Analyzer
    [Arguments]        ${uart_path}=sysbus.uart0
    Execute Command    showAnalyzer ${uart_path}

Wait For Boot String
    [Documentation]    Wait for a known boot banner on the UART.
    [Arguments]        ${expected}    ${uart_path}=sysbus.uart0    ${timeout}=${DEFAULT_UART_TIMEOUT}
    Create Terminal Tester    ${uart_path}    timeout=${timeout}
    Execute Command    start
    Wait For Line On Uart    ${expected}    timeout=${timeout}

# ── Memory helpers ────────────────────────────────────────────────

Read Word
    [Arguments]        ${address}
    ${val}=            Execute Command    sysbus ReadDoubleWord ${address}
    [Return]           ${val}

Write Word
    [Arguments]        ${address}    ${value}
    Execute Command    sysbus WriteDoubleWord ${address} ${value}

Memory Region Should Be Accessible
    [Documentation]    Write a canary, read it back, assert match.
    [Arguments]        ${address}
    Execute Command    sysbus WriteDoubleWord ${address} 0xDEADBEEF
    ${val}=            Execute Command    sysbus ReadDoubleWord ${address}
    Should Contain     ${val}    0xDEADBEEF

# ── GDB helpers ───────────────────────────────────────────────────

Start GDB Server
    [Arguments]        ${port}=${DEFAULT_GDB_PORT}
    Execute Command    machine StartGdbServer ${port}

# ── Timing helpers ────────────────────────────────────────────────

Advance Time By
    [Documentation]    Advance virtual time by a given number of milliseconds.
    [Arguments]        ${ms}
    Execute Command    emulation RunFor "${ms}ms"
