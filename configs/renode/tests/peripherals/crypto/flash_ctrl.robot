*** Settings ***
Documentation     STM32WL_FlashCtrl isolation tests — exercises the option-byte
...               / RDP enforcement path the firmware will drive at
...               provisioning end (security_impl.md §B9).  Verifies KEYR /
...               OPTKEYR unlock state machines, OPTR-write gating, OPTSTRT
...               commit semantics, and the RDP-level decode.
...               Phase 5.4 of the STM32 Renode parity plan.
...               Tags: crypto, peripheral, unit, §5.4

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${PROJECT_ROOT}${/}configs${/}renode${/}tests${/}peripherals${/}crypto${/}test_flash_ctrl_platform.resc

${FLASH_BASE}        0x58004000
${FLASH_ACR}         0x58004000
${FLASH_KEYR}        0x58004008
${FLASH_OPTKEYR}     0x5800400C
${FLASH_SR}          0x58004010
${FLASH_CR}          0x58004014
${FLASH_OPTR}        0x58004020
${RDP_LEVEL}         0x580043F0
${OPTSTRT_HITS}      0x580043F4
${BAD_KEYS}          0x580043F8

${KEYR_KEY1}         0x45670123
${KEYR_KEY2}         0xCDEF89AB
${OPTKEYR_KEY1}      0x08192A3B
${OPTKEYR_KEY2}      0x4C5D6E7F

${CR_OPTSTRT}        0x20000
${CR_OBL_LAUNCH}     0x8000000

# OPTR with RDP=Level 1 / Level 2 patterns.
${OPTR_RDP_L1}       0x000000BB
${OPTR_RDP_L2}       0x000000CC
${OPTR_RDP_L0}       0x000000AA


*** Test Cases ***

5.4.1 Reset Leaves Both Locks Engaged And RDP Level 0
    [Setup]    Setup Platform
    ${cr}=    Execute Command    sysbus ReadDoubleWord ${FLASH_CR}
    ${v}=    Convert To Integer    ${cr.strip()}
    ${lock}=     Evaluate    (${v} >> 31) & 1
    ${optlock}=  Evaluate    (${v} >> 30) & 1
    Should Be Equal As Integers    ${lock}    1    msg=CR.LOCK not engaged at reset
    Should Be Equal As Integers    ${optlock}    1    msg=CR.OPTLOCK not engaged at reset
    ${rdp}=    Execute Command    sysbus ReadDoubleWord ${RDP_LEVEL}
    Should Be Equal As Integers    ${rdp.strip()}    0
    ${optr}=    Execute Command    sysbus ReadDoubleWord ${FLASH_OPTR}
    Should Be Equal As Integers    ${optr.strip()}    0xAA
    [Teardown]    Reset Emulation

5.4.2 KEYR Unlock Sequence Clears CR.LOCK
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${FLASH_KEYR} ${KEYR_KEY1}
    Execute Command    sysbus WriteDoubleWord ${FLASH_KEYR} ${KEYR_KEY2}
    ${cr}=    Execute Command    sysbus ReadDoubleWord ${FLASH_CR}
    ${v}=    Convert To Integer    ${cr.strip()}
    ${lock}=    Evaluate    (${v} >> 31) & 1
    Should Be Equal As Integers    ${lock}    0    msg=CR.LOCK still set after correct KEYR sequence
    [Teardown]    Reset Emulation

5.4.3 Wrong KEYR Key Re-Locks And Latches PGSERR
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${FLASH_KEYR} ${KEYR_KEY1}
    Execute Command    sysbus WriteDoubleWord ${FLASH_KEYR} 0xDEADBEEF
    ${cr}=    Execute Command    sysbus ReadDoubleWord ${FLASH_CR}
    ${v}=    Convert To Integer    ${cr.strip()}
    ${lock}=    Evaluate    (${v} >> 31) & 1
    Should Be Equal As Integers    ${lock}    1
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${FLASH_SR}
    ${sv}=    Convert To Integer    ${sr.strip()}
    ${pgserr}=    Evaluate    (${sv} >> 7) & 1
    Should Be Equal As Integers    ${pgserr}    1
    ${bk}=    Execute Command    sysbus ReadDoubleWord ${BAD_KEYS}
    Should Be Equal As Integers    ${bk.strip()}    1
    [Teardown]    Reset Emulation

5.4.4 Full Unlock-Then-Commit Path Sets RDP Level 1
    [Documentation]    The exact sequence Wle5Pka.enable_rdp_level_1 will drive:
    ...                  1. KEYR  unlock (KEY1, KEY2)
    ...                  2. OPTKEYR unlock (KEY1, KEY2)
    ...                  3. Write 0xBB to OPTR
    ...                  4. Set CR.OPTSTRT
    ...                Then read back the RDP level magic register.
    [Setup]    Setup Platform
    Run RDP Enable Sequence    ${OPTR_RDP_L1}
    ${rdp}=    Execute Command    sysbus ReadDoubleWord ${RDP_LEVEL}
    Should Be Equal As Integers    ${rdp.strip()}    1
    ${hits}=    Execute Command    sysbus ReadDoubleWord ${OPTSTRT_HITS}
    Should Be Equal As Integers    ${hits.strip()}    1
    [Teardown]    Reset Emulation

5.4.5 OPTR Write While OPTLOCK Set Is Ignored
    [Documentation]    A bare OPTR write without unlocking OPTKEYR first
    ...                must NOT advance the staged value: subsequent OPTSTRT
    ...                must keep RDP at Level 0.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${FLASH_OPTR} ${OPTR_RDP_L1}
    Execute Command    sysbus WriteDoubleWord ${FLASH_KEYR} ${KEYR_KEY1}
    Execute Command    sysbus WriteDoubleWord ${FLASH_KEYR} ${KEYR_KEY2}
    Execute Command    sysbus WriteDoubleWord ${FLASH_OPTKEYR} ${OPTKEYR_KEY1}
    Execute Command    sysbus WriteDoubleWord ${FLASH_OPTKEYR} ${OPTKEYR_KEY2}
    Execute Command    sysbus WriteDoubleWord ${FLASH_CR} ${CR_OPTSTRT}
    ${rdp}=    Execute Command    sysbus ReadDoubleWord ${RDP_LEVEL}
    Should Be Equal As Integers    ${rdp.strip()}    0    msg=Pre-unlock OPTR write leaked through
    [Teardown]    Reset Emulation

5.4.6 RDP=0xCC Decodes To Level 2
    [Setup]    Setup Platform
    Run RDP Enable Sequence    ${OPTR_RDP_L2}
    ${rdp}=    Execute Command    sysbus ReadDoubleWord ${RDP_LEVEL}
    Should Be Equal As Integers    ${rdp.strip()}    2
    [Teardown]    Reset Emulation

5.4.7 Committed OPTR Persists After OPTLOCK Re-Engages
    [Documentation]    OPTSTRT must commit the staged OPTR AND re-set OPTLOCK
    ...                so a subsequent bare OPTR write cannot downgrade RDP.
    [Setup]    Setup Platform
    Run RDP Enable Sequence    ${OPTR_RDP_L1}
    Execute Command    sysbus WriteDoubleWord ${FLASH_OPTR} ${OPTR_RDP_L0}
    Execute Command    sysbus WriteDoubleWord ${FLASH_CR} ${CR_OPTSTRT}
    ${rdp}=    Execute Command    sysbus ReadDoubleWord ${RDP_LEVEL}
    Should Be Equal As Integers    ${rdp.strip()}    1    msg=RDP downgrade leaked through re-locked OPTLOCK
    [Teardown]    Reset Emulation


*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    logLevel 3

Run RDP Enable Sequence
    [Arguments]    ${optr_value}
    Execute Command    sysbus WriteDoubleWord ${FLASH_KEYR} ${KEYR_KEY1}
    Execute Command    sysbus WriteDoubleWord ${FLASH_KEYR} ${KEYR_KEY2}
    Execute Command    sysbus WriteDoubleWord ${FLASH_OPTKEYR} ${OPTKEYR_KEY1}
    Execute Command    sysbus WriteDoubleWord ${FLASH_OPTKEYR} ${OPTKEYR_KEY2}
    Execute Command    sysbus WriteDoubleWord ${FLASH_OPTR} ${optr_value}
    Execute Command    sysbus WriteDoubleWord ${FLASH_CR} ${CR_OPTSTRT}
