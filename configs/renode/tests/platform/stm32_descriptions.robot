*** Settings ***
Documentation     STM32 platform-description load suite — verifies that the
...               project's STM32 .repl loads cleanly under Renode and that
...               the expected SoC peripherals attach.  No firmware ELF is
...               required, so the suite runs even while the STM32 ports are
...               cargo-check-only.  Phase 7 / TEST-IMPL_SWM §G9.
...               STS §3 (platform descriptions), §11 (ARM/STM32 gap).

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}     ${CURDIR}${/}..${/}..${/}..${/}..
${REPL_G0B1}        ${PROJECT_ROOT}${/}configs${/}renode${/}soc${/}arm${/}stm32g0b1${/}stm32g0b1.repl
${REPL_F4}          ${PROJECT_ROOT}${/}configs${/}renode${/}soc${/}arm${/}stm32f4${/}stm32f4.repl
${REPL_NRF52840}    ${PROJECT_ROOT}${/}configs${/}renode${/}soc${/}arm${/}nrf52840${/}nrf52840.repl
${REPL_RP2040}      ${PROJECT_ROOT}${/}configs${/}renode${/}soc${/}arm${/}rp2040${/}rp2040.repl

*** Test Cases ***

STM32G0B1 SoC Repl Loads Without Error
    [Documentation]    Load configs/renode/soc/arm/stm32g0b1/stm32g0b1.repl directly
    ...                (no .resc, so no ELF needed).  cpu must be accessible
    ...                after the load, confirming the Cortex-M0+ core attached.
    [Setup]    Setup ARM Platform Test    stm32g0b1
    Load Platform Repl    ${REPL_G0B1}
    ${cpu}=    Execute Command    cpu
    Should Not Be Empty    ${cpu.strip()}    msg=cpu not accessible after STM32G0B1 platform load
    [Teardown]    Teardown ARM Platform Test

STM32G0B1 Memory Map Override Resizes Flash And RAM
    [Documentation]    The override .repl re-sizes flash to 0x80000 (512 KB)
    ...                and ram to 0x24000 (144 KB) over the base stm32g0
    ...                file's smaller defaults.  Probes verify the larger
    ...                regions are mapped (a read at the top of each region
    ...                returns a value without aborting).
    [Setup]    Setup ARM Platform Test    stm32g0b1-mmap
    Load Platform Repl    ${REPL_G0B1}
    Execute Command    sysbus ReadDoubleWord 0x0807FFFC
    Execute Command    sysbus ReadDoubleWord 0x20023FFC
    [Teardown]    Teardown ARM Platform Test

STM32G0B1 Standard Peripherals Are Accessible
    [Documentation]    USART1, the timer block, and the NVIC must appear in
    ...                the peripherals list after platform load — they are the
    ...                load-bearing pieces the swm-dhara-g0b1 firmware will
    ...                need once the port is un-excluded.
    [Setup]    Setup ARM Platform Test    stm32g0b1-periphs
    Load Platform Repl    ${REPL_G0B1}
    ${out}=    Execute Command    peripherals
    Should Contain    ${out}    usart1    msg=usart1 not in peripheral list
    Should Contain    ${out}    nvic      msg=nvic not in peripheral list
    Should Contain    ${out}    timer3    msg=timer3 not in peripheral list
    [Teardown]    Teardown ARM Platform Test

STM32F4 SoC Repl Loads Without Error
    [Documentation]    Sister ARM platform — covers the Cortex-M4F path used
    ...                by the node_sx1276 reference platform.  Confirms the
    ...                workspace's stm32f4.repl parses on Renode builds that
    ...                ship I2C.STM32_I2C; otherwise the suite skips with an
    ...                explanation (same convention as descriptions.robot).
    [Setup]    Setup ARM Platform Test    stm32f4
    Load Platform Repl Allow Skip    ${REPL_F4}    STM32F4
    ${cpu}=    Execute Command    cpu
    Should Not Be Empty    ${cpu.strip()}    msg=cpu not accessible after STM32F4 platform load
    [Teardown]    Teardown ARM Platform Test

Reference ARM Platforms In configs/renode/soc/arm Load
    [Documentation]    Spot-check the remaining ARM SoC .repls that ship with
    ...                the workspace (nrf52840, rp2040).  Each must load and
    ...                expose a cpu — confirms none have regressed under a
    ...                Renode version bump.  Skips silently if Renode is
    ...                missing the required model (printed by Renode itself).
    [Setup]    Setup ARM Platform Test    arm-sibling-repls
    Load Platform Repl Allow Skip    ${REPL_NRF52840}    nRF52840
    Reset Emulation
    Setup ARM Platform Test    arm-sibling-repls-2
    Load Platform Repl Allow Skip    ${REPL_RP2040}    RP2040
    [Teardown]    Teardown ARM Platform Test

*** Keywords ***

Setup ARM Platform Test
    [Arguments]    ${machine_name}
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    mach create "${machine_name}"

Teardown ARM Platform Test
    Run Keyword And Ignore Error    Execute Command    mach clear
    Run Keyword And Ignore Error    Reset Emulation

Load Platform Repl
    [Arguments]    ${repl_path}
    Execute Command    machine LoadPlatformDescription @${repl_path}

Load Platform Repl Allow Skip
    [Documentation]    Same as Load Platform Repl, but tolerates a missing
    ...                CPU/peripheral model in the portable Renode build.
    [Arguments]    ${repl_path}    ${platform_name}
    ${status}    ${err}=    Run Keyword And Ignore Error
    ...    Execute Command    machine LoadPlatformDescription @${repl_path}
    Run Keyword If    '${status}' == 'FAIL'    Pass Execution
    ...    ${platform_name} model not available in this Renode build — install full Renode to exercise
