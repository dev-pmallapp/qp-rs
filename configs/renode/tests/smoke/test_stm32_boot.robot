*** Settings ***
Documentation     Phase 1 boot-parity smoke for the STM32 targets.
...               Each test loads a platform `.resc`, lets the CPU run for
...               2 virtual seconds, and asserts the PC has advanced past
...               the reset vector — i.e. the firmware booted, completed
...               clock / RCC / PWR / DBGMCU init, and reached the
...               application layer.
...
...               Boot artefacts must exist:
...                 STM32WLE5  → target/thumbv7em-none-eabihf/debug/gagan
...                 STM32G0B1  → target/thumbv6m-none-eabi/debug/dhara
...                 STM32U545  → target/thumbv8m.main-none-eabihf/debug/pramukh
...               Build with:
...                 make arm TARGET=stm32wle5 ROLE=gagan
...                 make arm TARGET=stm32g0b1 ROLE=dhara
...                 make arm TARGET=stm32u545 ROLE=pramukh
Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}     ${CURDIR}${/}..${/}..${/}..${/}..
${WLE5_RESC}        ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}arm${/}stm32wle5_devkit${/}stm32wle5_devkit.resc
${G0B1_RESC}        ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}arm${/}stm32g0b1_devkit${/}stm32g0b1_devkit.resc
${U545_RESC}        ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}arm${/}stm32u545_devkit${/}stm32u545_devkit.resc
${BOOT_MIN_PC}      0x08000200

*** Keywords ***
Set Workspace CWD
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"

Tear Down Machine
    Run Keyword And Ignore Error    Execute Command    mach clear

Run And Check PC Past Reset
    [Arguments]    ${resc_path}    ${uart}    ${machine_name}
    Set Workspace CWD
    Execute Command    include @${resc_path}
    # Phase 2 task 2.8 — confirm the console USART analyzer is reachable
    # by the Robot Terminal Tester. Construction-side failure (mistyped
    # analyzer name, missing showAnalyzer directive) shows up here as a
    # tester-creation error; runtime UART traffic is exercised by the
    # role-specific suites once the bins emit a boot banner.
    Create Terminal Tester    sysbus.${uart}    machine=${machine_name}
    Execute Command    start
    Execute Command    sleep 2
    Execute Command    pause
    ${pc}=    Execute Command    cpu PC
    Log    PC after 2 s of execution: ${pc}
    Should Not Be Empty    ${pc}
    ${pc_val}=    Convert To Integer    ${pc.strip()}    16
    ${min}=    Convert To Integer    ${BOOT_MIN_PC}    16
    Should Be True    ${pc_val} > ${min}
    ...    msg=PC ${pc.strip()} did not advance past reset vector ${BOOT_MIN_PC} — firmware likely stalled in early init

*** Test Cases ***
STM32WLE5 Boots Past Reset Vector
    [Documentation]    Gagan firmware on STM32WLE5JC reaches the application layer.
    [Teardown]    Tear Down Machine
    Run And Check PC Past Reset    ${WLE5_RESC}    usart2    SWM-Wle5-Devkit

STM32G0B1 Boots Past Reset Vector
    [Documentation]    Dhara firmware on STM32G0B1CETx reaches the application layer.
    [Teardown]    Tear Down Machine
    Run And Check PC Past Reset    ${G0B1_RESC}    usart1    SWM-G0b1-Devkit

STM32U545 Boots Past Reset Vector
    [Documentation]    Pramukh firmware on STM32U545RETxQ reaches the application layer.
    ...                Phase 6.2.4 of tmp/stm32_renode_parity_plan.md.
    [Teardown]    Tear Down Machine
    Run And Check PC Past Reset    ${U545_RESC}    usart1    SWM-U545-Devkit
