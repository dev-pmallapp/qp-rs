*** Settings ***
Documentation     Parametric boot-parity smoke for every supported target.
...               One suite, three platforms — selected at runtime via
...               --variable TARGET_RESOURCE:configs/renode/tests/_targets/<target>.resource.
...
...               Phase 4.1 of the STM32 Renode parity plan: this is the
...               first suite written against the per-target resource files.
...               Boot artefacts for the picked TARGET / ROLE must exist:
...                 make esp ROLE=gagan                  (TARGET=esp32c6)
...                 make arm TARGET=stm32wle5 ROLE=gagan
...                 make arm TARGET=stm32g0b1 ROLE=dhara
Resource          ${RENODEKEYWORDS}
Resource          ${TARGET_RESOURCE}

*** Variables ***
# Default to esp32c6 if the caller did not pass a TARGET_RESOURCE; this
# keeps the suite runnable in isolation (`renode-test ...parametric_boot.robot`)
# without requiring the matrix shim.  Override with:
#   renode-test --variable TARGET_RESOURCE:configs/renode/tests/_targets/stm32wle5.resource ...
${TARGET_RESOURCE}    ${CURDIR}${/}..${/}_targets${/}esp32c6.resource

# ROLE selects which firmware bin to load.  The resource file exposes
# FW_<ROLE> variables; this default matches each target's first-class
# bring-up bin and can be overridden with --variable ROLE:talab.
${ROLE}               gagan

*** Keywords ***
Set Workspace CWD
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"

Tear Down Machine
    Run Keyword And Ignore Error    Execute Command    mach clear

Select Firmware For Role
    [Documentation]    Returns the absolute path of the ELF to load for
    ...                ${ROLE} on the selected target.  ${FW_<ROLE>} is
    ...                defined in _targets/<target>.resource.
    [Arguments]    ${role}
    ${upper}=    Convert To Upper Case    ${role}
    ${fw}=       Get Variable Value    \${FW_${upper}}
    Should Not Be Empty    ${fw}
    ...    msg=No firmware path for ROLE=${role} on target ${TARGET_ID}; check _targets/${TARGET_ID}.resource
    [Return]    ${fw}

*** Test Cases ***
Target Boots Past Reset Vector
    [Documentation]    Smoke: load the selected target's platform .resc,
    ...                load the ${ROLE} ELF, run 2 virtual seconds, assert
    ...                PC advanced past the reset vector — i.e. firmware
    ...                reached the application layer.
    [Teardown]    Tear Down Machine

    ${fw}=    Select Firmware For Role    ${ROLE}
    Set Workspace CWD
    Execute Command    $bin = @${fw}
    Execute Command    include @${TARGET_PLATFORM_RESC}

    Create Terminal Tester    ${TARGET_CONSOLE_PATH}    machine=${TARGET_MACHINE_NAME}

    Execute Command    start
    Execute Command    sleep 2
    Execute Command    pause

    ${pc}=    Execute Command    cpu PC
    Log    PC after 2 s of execution on ${TARGET_ID}/${ROLE}: ${pc}
    Should Not Be Empty    ${pc}

    ${pc_val}=    Convert To Integer    ${pc.strip()}    16
    ${min}=       Convert To Integer    ${BOOT_MIN_PC}    16
    Should Be True    ${pc_val} > ${min}
    ...    msg=PC ${pc.strip()} did not advance past reset vector ${BOOT_MIN_PC} on ${TARGET_ID}/${ROLE}
