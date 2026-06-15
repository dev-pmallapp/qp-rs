*** Settings ***
Documentation     STM32WL_AES1 isolation tests — drives the C# model via sysbus
...               reads/writes, no firmware running.  Verifies register decode
...               and AES-128 ECB / CBC / CTR results against published NIST
...               test vectors (FIPS-197 §C.1, SP 800-38A §F).
...               Phase 5.1 of the STM32 Renode parity plan.
...               Tags: crypto, peripheral, unit, §5.1

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${PROJECT_ROOT}${/}configs${/}renode${/}tests${/}peripherals${/}crypto${/}test_aes1_platform.resc
${AES_BASE}          0x58005000

# Register offsets
${AES_CR}            0x58005000
${AES_SR}            0x58005004
${AES_DINR}          0x58005008
${AES_DOUTR}         0x5800500C
${AES_KEYR0}         0x58005010
${AES_KEYR1}         0x58005014
${AES_KEYR2}         0x58005018
${AES_KEYR3}         0x5800501C
${AES_IVR0}          0x58005020
${AES_IVR1}          0x58005024
${AES_IVR2}          0x58005028
${AES_IVR3}          0x5800502C
${AES_OP_COUNT}      0x580053F0
${AES_FORCE_FAIL}    0x580053F4

# CR fields
${CR_EN}             0x1
${CR_MODE_DECRYPT}   0x10
${CR_CHMOD_ECB}      0x00
${CR_CHMOD_CBC}      0x20
${CR_CHMOD_CTR}      0x40
${CR_CCFC}           0x80
${SR_CCF}            0x1


*** Test Cases ***

5.1.1 Register Read After Reset Returns Defaults
    [Documentation]    Bare-load state: CR=0, SR=0, OP_COUNT=0.
    [Setup]    Setup Platform
    ${cr}=    Execute Command    sysbus ReadDoubleWord ${AES_CR}
    Should Be Equal As Integers    ${cr.strip()}    0
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${AES_SR}
    Should Be Equal As Integers    ${sr.strip()}    0
    ${oc}=    Execute Command    sysbus ReadDoubleWord ${AES_OP_COUNT}
    Should Be Equal As Integers    ${oc.strip()}    0
    [Teardown]    Reset Emulation

5.1.2 AES-128 ECB Encrypt Matches NIST FIPS-197 Vector
    [Documentation]    Key = 2b7e151628aed2a6abf7158809cf4f3c
    ...                PT  = 6bc1bee22e409f96e93d7e117393172a
    ...                CT  = 3ad77bb40d7a3660a89ecaf32466ef97
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR3} 0x2b7e1516
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR2} 0x28aed2a6
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR1} 0xabf71588
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR0} 0x09cf4f3c
    Execute Command    sysbus WriteDoubleWord ${AES_CR} ${CR_EN}
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x6bc1bee2
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x2e409f96
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0xe93d7e11
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x7393172a
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${AES_SR}
    ${v}=     Convert To Integer    ${sr.strip()}
    ${ccf}=   Evaluate    ${v} & 1
    Should Be Equal As Integers    ${ccf}    1    msg=CCF not asserted after 4 DINR writes
    ${w0}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w1}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w2}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w3}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    Should Be Equal As Integers    ${w0.strip()}    0x3ad77bb4
    Should Be Equal As Integers    ${w1.strip()}    0x0d7a3660
    Should Be Equal As Integers    ${w2.strip()}    0xa89ecaf3
    Should Be Equal As Integers    ${w3.strip()}    0x2466ef97
    [Teardown]    Reset Emulation

5.1.3 AES-128 ECB Decrypt Round-Trip
    [Documentation]    Decrypt the FIPS-197 ciphertext and check the plaintext returns.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR3} 0x2b7e1516
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR2} 0x28aed2a6
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR1} 0xabf71588
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR0} 0x09cf4f3c
    ${cr}=    Evaluate    ${CR_EN} | ${CR_MODE_DECRYPT}
    Execute Command    sysbus WriteDoubleWord ${AES_CR} ${cr}
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x3ad77bb4
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x0d7a3660
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0xa89ecaf3
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x2466ef97
    ${w0}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w1}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w2}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w3}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    Should Be Equal As Integers    ${w0.strip()}    0x6bc1bee2
    Should Be Equal As Integers    ${w1.strip()}    0x2e409f96
    Should Be Equal As Integers    ${w2.strip()}    0xe93d7e11
    Should Be Equal As Integers    ${w3.strip()}    0x7393172a
    [Teardown]    Reset Emulation

5.1.4 AES-128 CBC Encrypt Matches NIST SP 800-38A Vector
    [Documentation]    Key = 2b7e151628aed2a6abf7158809cf4f3c
    ...                IV  = 000102030405060708090a0b0c0d0e0f
    ...                PT  = 6bc1bee22e409f96e93d7e117393172a
    ...                CT  = 7649abac8119b246cee98e9b12e9197d
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR3} 0x2b7e1516
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR2} 0x28aed2a6
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR1} 0xabf71588
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR0} 0x09cf4f3c
    Execute Command    sysbus WriteDoubleWord ${AES_IVR3} 0x00010203
    Execute Command    sysbus WriteDoubleWord ${AES_IVR2} 0x04050607
    Execute Command    sysbus WriteDoubleWord ${AES_IVR1} 0x08090a0b
    Execute Command    sysbus WriteDoubleWord ${AES_IVR0} 0x0c0d0e0f
    ${cr}=    Evaluate    ${CR_EN} | ${CR_CHMOD_CBC}
    Execute Command    sysbus WriteDoubleWord ${AES_CR} ${cr}
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x6bc1bee2
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x2e409f96
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0xe93d7e11
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x7393172a
    ${w0}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w1}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w2}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w3}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    Should Be Equal As Integers    ${w0.strip()}    0x7649abac
    Should Be Equal As Integers    ${w1.strip()}    0x8119b246
    Should Be Equal As Integers    ${w2.strip()}    0xcee98e9b
    Should Be Equal As Integers    ${w3.strip()}    0x12e9197d
    [Teardown]    Reset Emulation

5.1.5 AES-128 CTR Matches NIST SP 800-38A Vector
    [Documentation]    Key       = 2b7e151628aed2a6abf7158809cf4f3c
    ...                Counter   = f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff
    ...                PT        = 6bc1bee22e409f96e93d7e117393172a
    ...                CT        = 874d6191b620e3261bef6864990db6ce
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR3} 0x2b7e1516
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR2} 0x28aed2a6
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR1} 0xabf71588
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR0} 0x09cf4f3c
    Execute Command    sysbus WriteDoubleWord ${AES_IVR3} 0xf0f1f2f3
    Execute Command    sysbus WriteDoubleWord ${AES_IVR2} 0xf4f5f6f7
    Execute Command    sysbus WriteDoubleWord ${AES_IVR1} 0xf8f9fafb
    Execute Command    sysbus WriteDoubleWord ${AES_IVR0} 0xfcfdfeff
    ${cr}=    Evaluate    ${CR_EN} | ${CR_CHMOD_CTR}
    Execute Command    sysbus WriteDoubleWord ${AES_CR} ${cr}
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x6bc1bee2
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x2e409f96
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0xe93d7e11
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0x7393172a
    ${w0}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w1}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w2}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${w3}=    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    Should Be Equal As Integers    ${w0.strip()}    0x874d6191
    Should Be Equal As Integers    ${w1.strip()}    0xb620e326
    Should Be Equal As Integers    ${w2.strip()}    0x1bef6864
    Should Be Equal As Integers    ${w3.strip()}    0x990db6ce
    [Teardown]    Reset Emulation

5.1.6 OP_COUNT Increments On Each Completed Block
    [Documentation]    Two blocks back-to-back must bump OP_COUNT to 2.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR3} 0
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR2} 0
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR1} 0
    Execute Command    sysbus WriteDoubleWord ${AES_KEYR0} 0
    Execute Command    sysbus WriteDoubleWord ${AES_CR} ${CR_EN}
    Repeat Keyword    2 times    Push One Plaintext Block And Drain
    ${oc}=    Execute Command    sysbus ReadDoubleWord ${AES_OP_COUNT}
    Should Be Equal As Integers    ${oc.strip()}    2
    [Teardown]    Reset Emulation

5.1.7 FORCE_FAIL Latches SR.RDERR On The Next DOUTR Read
    [Documentation]    Robot test surface for crypto error paths.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${AES_FORCE_FAIL} 1
    Execute Command    sysbus WriteDoubleWord ${AES_CR} ${CR_EN}
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0
    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${AES_SR}
    ${v}=     Convert To Integer    ${sr.strip()}
    ${rderr}=    Evaluate    (${v} >> 1) & 1
    Should Be Equal As Integers    ${rderr}    1    msg=RDERR did not latch after FORCE_FAIL
    [Teardown]    Reset Emulation


*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    logLevel 3

Push One Plaintext Block And Drain
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0
    Execute Command    sysbus WriteDoubleWord ${AES_DINR} 0
    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
    Execute Command    sysbus ReadDoubleWord ${AES_DOUTR}
