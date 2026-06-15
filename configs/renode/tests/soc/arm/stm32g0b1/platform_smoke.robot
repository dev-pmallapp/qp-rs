*** Settings ***
Documentation     STM32G0B1 SoC harness smoke suite — mirror of
...               tests/soc/esp32c6/runfor_hooks.robot for the ARM column.
...               Runs without firmware (the swm-dhara-g0b1 binary is
...               cargo-check-only today, STS §11), so the suite exercises
...               only what the .repl and Renode's emulation engine give us:
...               variable scratchpad, RAM read/write, CPU register access,
...               and a deterministic RunFor that does not crash the model.
...               Phase 7 / TEST-IMPL_SWM §G9.

Resource          ${RENODEKEYWORDS}
Library           String

*** Variables ***
${PROJECT_ROOT}     ${CURDIR}${/}..${/}..${/}..${/}..${/}..${/}..
${REPL_G0B1}        ${PROJECT_ROOT}${/}configs${/}renode${/}soc${/}arm${/}stm32g0b1${/}stm32g0b1.repl
# Scratch words inside the overridden 144 KB SRAM region.
${SCRATCH_LOW}      0x20000000
${SCRATCH_HIGH}     0x20023FF0

*** Keywords ***
Load Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    mach create "stm32g0b1-soc-smoke"
    Execute Command    machine LoadPlatformDescription @${REPL_G0B1}
    Execute Command    logLevel 0

Clear Machine
    Run Keyword And Ignore Error    Execute Command    mach clear
    Run Keyword And Ignore Error    Reset Emulation

*** Test Cases ***
Monitor Variables Read And Write
    [Documentation]    Sanity check the Monitor scripting layer — same
    ...                contract the ESP32-C6 SoC suite asserts.
    [Setup]    Load Platform
    Execute Command    set test_flag 0
    ${v}=    Execute Command    echo $test_flag
    Should Be Equal As Integers    ${v.strip()}    0
    Execute Command    set test_flag 42
    ${v}=    Execute Command    echo $test_flag
    Should Be Equal As Integers    ${v.strip()}    42
    [Teardown]    Clear Machine

SRAM Is Readable And Writable
    [Documentation]    The override .repl re-sizes ram to 0x24000 (144 KB).
    ...                Touch the bottom and the top of the region — anything
    ...                else means the override silently fell back to the
    ...                base stm32g0 size (0xC000 = 48 KB).
    [Setup]    Load Platform
    Execute Command    sysbus WriteDoubleWord ${SCRATCH_LOW} 0xCAFEBABE
    ${v}=    Execute Command    sysbus ReadDoubleWord ${SCRATCH_LOW}
    Should Be Equal As Integers    ${v.strip()}    0xCAFEBABE
    Execute Command    sysbus WriteDoubleWord ${SCRATCH_HIGH} 0xDEADBEEF
    ${v}=    Execute Command    sysbus ReadDoubleWord ${SCRATCH_HIGH}
    Should Be Equal As Integers    ${v.strip()}    0xDEADBEEF
    [Teardown]    Clear Machine

Cortex M0 CPU Registers Are Accessible
    [Documentation]    The CPU is exposed as `cpu` (not `cpu0` as on the
    ...                ESP32-C6 dual-core platform).  PC and SP must be
    ...                accessible — confirms the Cortex-M0+ core attached
    ...                and the platform's NVIC linkage is intact.
    [Setup]    Load Platform
    ${pc}=    Execute Command    cpu PC
    Should Not Be Empty    ${pc.strip()}    msg=cpu PC empty after STM32G0B1 platform load
    ${sp}=    Execute Command    cpu SP
    Should Not Be Empty    ${sp.strip()}    msg=cpu SP empty after STM32G0B1 platform load
    [Teardown]    Clear Machine

RunFor Without Firmware Does Not Crash The Model
    [Documentation]    No ELF is loaded, so the CPU executes whatever the
    ...                vector table at 0x08000000 (zeroed flash) points to.
    ...                The assertion is purely that Renode does not abort
    ...                during a brief virtual-time advance — proves the
    ...                platform is self-consistent enough to step.
    [Setup]    Load Platform
    Execute Command    emulation RunFor "00:00:00.01"
    ${out}=    Execute Command    machine
    Should Not Be Empty    ${out.strip()}    msg=machine handle lost after RunFor
    [Teardown]    Clear Machine
