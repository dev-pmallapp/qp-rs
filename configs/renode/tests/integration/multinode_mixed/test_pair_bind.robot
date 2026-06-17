*** Settings ***
Documentation     Mixed-SoC pair-matrix smoke (Phase 6.3.2 of
...               tmp/stm32_renode_parity_plan.md).
...
...               One parametric test case: generate the per-pair .resc
...               via scripts/gen-multinode-resc.sh for the given
...               (A_TARGET, A_ROLE, B_TARGET, B_ROLE), include it, run
...               both endpoints for a short window, and assert each PC
...               has advanced past its reset vector.
...
...               CI drives the matrix via --variable assignments:
...                 renode-test \\
...                   --variable A_TARGET:stm32u545 --variable A_ROLE:pramukh \\
...                   --variable B_TARGET:stm32wle5 --variable B_ROLE:gagan \\
...                   configs/renode/tests/integration/multinode_mixed/test_pair_bind.robot
...
...               Pre-reqs: each endpoint's role bin must be built —
...                 make arm  TARGET=stm32u545 ROLE=pramukh
...                 make arm  TARGET=stm32wle5 ROLE=gagan
...                 make esp  ROLE=pramukh                  (if A or B == esp32c6)
Library           Process
Library           OperatingSystem
Resource          ${RENODEKEYWORDS}
Test Timeout      90 seconds

*** Variables ***
${PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..${/}..

# Defaults so the suite can be smoke-run without --variable flags.  CI
# overrides these per matrix cell.
${A_TARGET}        stm32wle5
${A_ROLE}          gagan
${B_TARGET}        esp32c6
${B_ROLE}          pramukh

# Reset-vector minimum PCs.  STM32 flash is at 0x08000000; ESP32-C6
# starts in IROM at 0x40800000.
&{BOOT_MIN_PC}=    esp32c6=0x40800000    stm32wle5=0x08000200
...                stm32g0b1=0x08000200    stm32u545=0x08000200

&{CONSOLE_PATH}=   esp32c6=sysbus.usb_serial_jtag    stm32wle5=sysbus.usart2
...                stm32g0b1=sysbus.usart1            stm32u545=sysbus.usart1

# ESP32-C6's CortexM-equivalent RISC-V CPU is registered as cpu0 in
# esp32c6.repl; the STM32 SoC repls declare the M3/M4/M0+/M33 core as
# plain `cpu`.  Pick the right name per target so `cpu0 PC` is queried
# on C6 endpoints and `cpu PC` on STM32 endpoints.
&{CPU_DEVICE}=     esp32c6=cpu0    stm32wle5=cpu    stm32g0b1=cpu    stm32u545=cpu

*** Keywords ***
Set Workspace CWD
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"

Tear Down Pair
    Run Keyword And Ignore Error    Execute Command    mach clear

Generate Pair Resc
    [Arguments]    ${a_target}    ${a_role}    ${b_target}    ${b_role}
    ${result}=    Run Process    scripts/gen-multinode-resc.sh
    ...                          ${a_target}    ${a_role}    ${b_target}    ${b_role}
    ...                          cwd=${PROJECT_ROOT}
    Should Be Equal As Integers    ${result.rc}    0
    ...    msg=gen-multinode-resc.sh failed: ${result.stderr}
    ${out_path}=    Set Variable    ${PROJECT_ROOT}${/}configs${/}renode${/}swm${/}generated${/}multinode_${a_target}_${a_role}__${b_target}_${b_role}.resc
    File Should Exist    ${out_path}
    RETURN    ${out_path}

Assert PC Past
    [Arguments]    ${machine}    ${pc_text}    ${min_hex}
    Should Not Be Empty    ${pc_text}
    ...    msg=cpu PC returned empty for ${machine} — startup never completed
    ${pc_val}=    Convert To Integer    ${pc_text.strip()}    16
    ${min}=       Convert To Integer    ${min_hex}    16
    Should Be True    ${pc_val} > ${min}
    ...    msg=${machine} PC ${pc_text.strip()} did not advance past ${min_hex}

*** Test Cases ***
Pair Boots Both Halves
    [Documentation]    Construction-side parity check across the four
    ...                LR1121 adapter variants.  Catches missing-class
    ...                regressions and platform-load errors that single-
    ...                target matrix cells can't see.
    [Tags]    sts-7.0    mixed-soc    lr1121-parity    pair-matrix
    [Teardown]    Tear Down Pair

    Set Workspace CWD
    ${resc}=    Generate Pair Resc    ${A_TARGET}    ${A_ROLE}    ${B_TARGET}    ${B_ROLE}

    ${a_machine}=    Set Variable    SWM-Pair-A-${A_TARGET}-${A_ROLE}
    ${b_machine}=    Set Variable    SWM-Pair-B-${B_TARGET}-${B_ROLE}

    Execute Command    include @${resc}

    Create Terminal Tester    ${CONSOLE_PATH}[${A_TARGET}]    machine=${a_machine}
    Create Terminal Tester    ${CONSOLE_PATH}[${B_TARGET}]    machine=${b_machine}

    Execute Command    start
    Execute Command    sleep 3
    Execute Command    pause

    Execute Command    mach set "${a_machine}"
    ${a_pc}=    Execute Command    ${CPU_DEVICE}[${A_TARGET}] PC
    Assert PC Past    ${a_machine}    ${a_pc}    ${BOOT_MIN_PC}[${A_TARGET}]

    Execute Command    mach set "${b_machine}"
    ${b_pc}=    Execute Command    ${CPU_DEVICE}[${B_TARGET}] PC
    Assert PC Past    ${b_machine}    ${b_pc}    ${BOOT_MIN_PC}[${B_TARGET}]
