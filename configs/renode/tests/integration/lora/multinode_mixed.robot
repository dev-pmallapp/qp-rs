*** Settings ***
Documentation     Mixed-SoC multi-node smoke (Phase 4.3 of the STM32 Renode
...               parity plan).  Loads the `swm_multinode_mixed.resc` script,
...               which spins up one STM32WLE5 machine (LR1121SubGhzRadio @
...               SUBGHZSPI) and one ESP32-C6 machine (LR1121Radio @ GPSPI2)
...               on a shared IEEE802_15_4 wireless medium, then asserts
...               each machine boots past its reset vector.
...
...               The IRadio surface is unified at the medium layer; this
...               test catches construction-side regressions where one of
...               the two LR1121 adapters fails to register, attach to the
...               medium, or honour the firmware's CPU-side bring-up.
...
...               Pre-reqs:
...                 make arm TARGET=stm32wle5 ROLE=gagan
...                 make esp ROLE=pramukh
...
...               The default orientation is Gagan-on-WLE5 ↔ Pramukh-on-C6;
...               override $wle5_fw / $c6_fw via `Execute Command` lines to
...               run the reverse pairing.
Resource          ${RENODEKEYWORDS}
Test Timeout      60 seconds

*** Variables ***
${PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${MIXED_RESC}      ${PROJECT_ROOT}${/}configs${/}renode${/}swm${/}swm_multinode_mixed.resc

${WLE5_MACHINE}    SWM-Mixed-Wle5
${C6_MACHINE}      SWM-Mixed-C6

${WLE5_CONSOLE}    sysbus.usart2
${C6_CONSOLE}      sysbus.usb_serial_jtag

${WLE5_BOOT_MIN_PC}    0x08000200
${C6_BOOT_MIN_PC}      0x40800000

*** Keywords ***
Set Workspace CWD
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"

Reset Both Machines
    Run Keyword And Ignore Error    Execute Command    mach clear

Assert PC Past
    [Arguments]    ${machine}    ${pc_text}    ${min_hex}
    Should Not Be Empty    ${pc_text}
    ...    msg=cpu PC returned empty for ${machine} — startup never completed
    ${pc_val}=    Convert To Integer    ${pc_text.strip()}    16
    ${min}=       Convert To Integer    ${min_hex}    16
    Should Be True    ${pc_val} > ${min}
    ...    msg=${machine} PC ${pc_text.strip()} did not advance past ${min_hex}

*** Test Cases ***
Mixed SoC Multinode Boots Both Halves
    [Documentation]    Construction-side parity check between the two
    ...                LR1121 adapter variants. Catches missing-class
    ...                regressions (e.g. LR1121SubGhzRadio not loaded
    ...                before the medium attach) and platform-load errors
    ...                that would otherwise only surface in a long-running
    ...                TX/RX integration test.
    [Tags]    sts-7.0    mixed-soc    lr1121-parity
    [Teardown]    Reset Both Machines

    Set Workspace CWD
    Execute Command    include @${MIXED_RESC}

    Create Terminal Tester    ${WLE5_CONSOLE}    machine=${WLE5_MACHINE}
    Create Terminal Tester    ${C6_CONSOLE}      machine=${C6_MACHINE}

    Execute Command    start
    Execute Command    sleep 3
    Execute Command    pause

    Execute Command    mach set "${WLE5_MACHINE}"
    ${wle5_pc}=    Execute Command    cpu PC
    Assert PC Past    ${WLE5_MACHINE}    ${wle5_pc}    ${WLE5_BOOT_MIN_PC}

    Execute Command    mach set "${C6_MACHINE}"
    ${c6_pc}=    Execute Command    cpu PC
    Assert PC Past    ${C6_MACHINE}    ${c6_pc}    ${C6_BOOT_MIN_PC}
