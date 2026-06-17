*** Settings ***
Documentation     STM32WL_PKA isolation tests — drives the C# PKA model via
...               sysbus reads/writes.  Verifies register protocol (CR / SR /
...               CLRFR), PKA_RAM addressing, magic register surface, and
...               P-256 scalar multiplication against a known test vector
...               (2 * G with G = P-256 generator).
...               Phase 5.2 of the STM32 Renode parity plan.
...               Tags: crypto, peripheral, unit, §5.2

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${PROJECT_ROOT}${/}configs${/}renode${/}tests${/}peripherals${/}crypto${/}test_pka_platform.resc

${PKA_BASE}          0x58002000
${PKA_CR}            0x58002000
${PKA_SR}            0x58002004
${PKA_CLRFR}         0x58002008
${PKA_OP_COUNT}      0x580037F0
${PKA_FORCE_FAIL}    0x580037F4
${PKA_LAST_MODE}     0x580037F8
${PKA_LAST_ERR}      0x580037FC

# RAM operand bases — PKA_BASE + {ECC_K, X, Y} HAL offsets.
${RAM_K}             0x58002508
${RAM_PX}            0x5800255C
${RAM_PY}            0x580025B0

# Mode codes shifted into CR bits 8..13 with EN | START
${CR_ECDH}           0x2203
${CR_ECDSA_SIGN}     0x2403
${CR_ECDSA_VRFY}     0x2603
${CR_BOGUS}          0x3F03

# SR bits
${SR_PROCEND}        0x20000
${SR_RAMERR}         0x80000


*** Test Cases ***

5.2.1 Reset Leaves CR=0 SR=0 OP_COUNT=0
    [Setup]    Setup Platform
    ${cr}=    Execute Command    sysbus ReadDoubleWord ${PKA_CR}
    Should Be Equal As Integers    ${cr.strip()}    0
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${PKA_SR}
    Should Be Equal As Integers    ${sr.strip()}    0
    ${oc}=    Execute Command    sysbus ReadDoubleWord ${PKA_OP_COUNT}
    Should Be Equal As Integers    ${oc.strip()}    0
    [Teardown]    Reset Emulation

5.2.2 PKA_RAM Round-Trip
    [Documentation]    Write/read a 32-bit value at the K operand offset.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${RAM_K} 0xDEADBEEF
    ${v}=    Execute Command    sysbus ReadDoubleWord ${RAM_K}
    Should Be Equal As Integers    ${v.strip()}    0xDEADBEEF
    [Teardown]    Reset Emulation

5.2.3 Unsupported Mode Sets RAMERR And LAST_ERR
    [Documentation]    Mode 0x3F is reserved.  Issuing START with that
    ...                MODE must set SR.RAMERR, latch SR.PROCEND, and set
    ...                LAST_ERR=1 (ERR_UNSUPPORTED).
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${PKA_CR} ${CR_BOGUS}
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${PKA_SR}
    ${v}=    Convert To Integer    ${sr.strip()}
    ${procend}=    Evaluate    (${v} >> 17) & 1
    ${ramerr}=     Evaluate    (${v} >> 19) & 1
    Should Be Equal As Integers    ${procend}    1
    Should Be Equal As Integers    ${ramerr}     1
    ${err}=    Execute Command    sysbus ReadDoubleWord ${PKA_LAST_ERR}
    Should Be Equal As Integers    ${err.strip()}    1
    [Teardown]    Reset Emulation

5.2.4 CLRFR Clears PROCEND And RAMERR
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${PKA_CR} ${CR_BOGUS}
    Execute Command    sysbus WriteDoubleWord ${PKA_CLRFR} 0xA0000
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${PKA_SR}
    Should Be Equal As Integers    ${sr.strip()}    0
    [Teardown]    Reset Emulation

5.2.5 ECDSA Sign Mode Stubbed — RAMERR Plus LAST_ERR=4
    [Documentation]    Until real ECDSA modelling lands, mode 0x24 must
    ...                latch PROCEND, set RAMERR, and report ERR_NOT_MODELLED.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${PKA_CR} ${CR_ECDSA_SIGN}
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${PKA_SR}
    ${v}=    Convert To Integer    ${sr.strip()}
    ${procend}=    Evaluate    (${v} >> 17) & 1
    ${ramerr}=     Evaluate    (${v} >> 19) & 1
    Should Be Equal As Integers    ${procend}    1
    Should Be Equal As Integers    ${ramerr}     1
    ${err}=    Execute Command    sysbus ReadDoubleWord ${PKA_LAST_ERR}
    Should Be Equal As Integers    ${err.strip()}    4
    [Teardown]    Reset Emulation

5.2.6 ECDH P-256 — 2 * G Matches Published Doubled Generator
    [Documentation]    Load scalar k=2 and the P-256 generator G into the
    ...                ECDH operand slots, run mode 0x22, then check that
    ...                the result matches the known 2G coordinates from
    ...                NIST FIPS-186-4 Annex D (and SEC2 §2.7.2).
    ...                2G_x = 0x7cf27b188d034f7e8a52380304b51ac3c08969e277f21b35a60b48fc47669978
    ...                2G_y = 0x07775510db8ed040293d9ac69f7430dbba7dade63ce982299e04b79d227873d1
    [Setup]    Setup Platform
    Load Scalar Two At    ${RAM_K}
    Load P256 Generator
    Execute Command    sysbus WriteDoubleWord ${PKA_CR} ${CR_ECDH}
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${PKA_SR}
    ${v}=    Convert To Integer    ${sr.strip()}
    ${procend}=    Evaluate    (${v} >> 17) & 1
    ${ramerr}=     Evaluate    (${v} >> 19) & 1
    Should Be Equal As Integers    ${procend}    1
    Should Be Equal As Integers    ${ramerr}     0    msg=RAMERR set during ECDH; check operand layout
    ${err}=    Execute Command    sysbus ReadDoubleWord ${PKA_LAST_ERR}
    Should Be Equal As Integers    ${err.strip()}    0
    Check Result X Matches Doubled Generator
    Check Result Y Matches Doubled Generator
    [Teardown]    Reset Emulation

5.2.7 FORCE_FAIL Latches RAMERR On The Next Op
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${PKA_FORCE_FAIL} 1
    Execute Command    sysbus WriteDoubleWord ${PKA_CR} ${CR_ECDH}
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${PKA_SR}
    ${v}=    Convert To Integer    ${sr.strip()}
    ${ramerr}=    Evaluate    (${v} >> 19) & 1
    Should Be Equal As Integers    ${ramerr}    1
    ${err}=    Execute Command    sysbus ReadDoubleWord ${PKA_LAST_ERR}
    Should Be Equal As Integers    ${err.strip()}    3
    [Teardown]    Reset Emulation


*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    logLevel 3

Load Scalar Two At
    [Arguments]    ${addr}
    ${base}=    Convert To Integer    ${addr}
    Execute Command    sysbus WriteDoubleWord ${base} 0x00000002
    ${a1}=    Evaluate    ${base} + 0x04
    Execute Command    sysbus WriteDoubleWord ${a1} 0
    ${a2}=    Evaluate    ${base} + 0x08
    Execute Command    sysbus WriteDoubleWord ${a2} 0
    ${a3}=    Evaluate    ${base} + 0x0C
    Execute Command    sysbus WriteDoubleWord ${a3} 0
    ${a4}=    Evaluate    ${base} + 0x10
    Execute Command    sysbus WriteDoubleWord ${a4} 0
    ${a5}=    Evaluate    ${base} + 0x14
    Execute Command    sysbus WriteDoubleWord ${a5} 0
    ${a6}=    Evaluate    ${base} + 0x18
    Execute Command    sysbus WriteDoubleWord ${a6} 0
    ${a7}=    Evaluate    ${base} + 0x1C
    Execute Command    sysbus WriteDoubleWord ${a7} 0

Load P256 Generator
    # Gx = 0x6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296
    Execute Command    sysbus WriteDoubleWord 0x5800255C 0xd898c296
    Execute Command    sysbus WriteDoubleWord 0x58002560 0xf4a13945
    Execute Command    sysbus WriteDoubleWord 0x58002564 0x2deb33a0
    Execute Command    sysbus WriteDoubleWord 0x58002568 0x77037d81
    Execute Command    sysbus WriteDoubleWord 0x5800256C 0x63a440f2
    Execute Command    sysbus WriteDoubleWord 0x58002570 0xf8bce6e5
    Execute Command    sysbus WriteDoubleWord 0x58002574 0xe12c4247
    Execute Command    sysbus WriteDoubleWord 0x58002578 0x6b17d1f2
    # Gy = 0x4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5
    Execute Command    sysbus WriteDoubleWord 0x580025B0 0x37bf51f5
    Execute Command    sysbus WriteDoubleWord 0x580025B4 0xcbb64068
    Execute Command    sysbus WriteDoubleWord 0x580025B8 0x6b315ece
    Execute Command    sysbus WriteDoubleWord 0x580025BC 0x2bce3357
    Execute Command    sysbus WriteDoubleWord 0x580025C0 0x7c0f9e16
    Execute Command    sysbus WriteDoubleWord 0x580025C4 0x8ee7eb4a
    Execute Command    sysbus WriteDoubleWord 0x580025C8 0xfe1a7f9b
    Execute Command    sysbus WriteDoubleWord 0x580025CC 0x4fe342e2

Check Result X Matches Doubled Generator
    # 2G_x = 0x7cf27b188d034f7e8a52380304b51ac3c08969e277f21b35a60b48fc47669978
    ${w0}=    Execute Command    sysbus ReadDoubleWord 0x5800255C
    ${w1}=    Execute Command    sysbus ReadDoubleWord 0x58002560
    ${w2}=    Execute Command    sysbus ReadDoubleWord 0x58002564
    ${w3}=    Execute Command    sysbus ReadDoubleWord 0x58002568
    ${w4}=    Execute Command    sysbus ReadDoubleWord 0x5800256C
    ${w5}=    Execute Command    sysbus ReadDoubleWord 0x58002570
    ${w6}=    Execute Command    sysbus ReadDoubleWord 0x58002574
    ${w7}=    Execute Command    sysbus ReadDoubleWord 0x58002578
    Should Be Equal As Integers    ${w0.strip()}    0x47669978
    Should Be Equal As Integers    ${w1.strip()}    0xa60b48fc
    Should Be Equal As Integers    ${w2.strip()}    0x77f21b35
    Should Be Equal As Integers    ${w3.strip()}    0xc08969e2
    Should Be Equal As Integers    ${w4.strip()}    0x04b51ac3
    Should Be Equal As Integers    ${w5.strip()}    0x8a523803
    Should Be Equal As Integers    ${w6.strip()}    0x8d034f7e
    Should Be Equal As Integers    ${w7.strip()}    0x7cf27b18

Check Result Y Matches Doubled Generator
    # 2G_y = 0x07775510db8ed040293d9ac69f7430dbba7dade63ce982299e04b79d227873d1
    ${w0}=    Execute Command    sysbus ReadDoubleWord 0x580025B0
    ${w1}=    Execute Command    sysbus ReadDoubleWord 0x580025B4
    ${w2}=    Execute Command    sysbus ReadDoubleWord 0x580025B8
    ${w3}=    Execute Command    sysbus ReadDoubleWord 0x580025BC
    ${w4}=    Execute Command    sysbus ReadDoubleWord 0x580025C0
    ${w5}=    Execute Command    sysbus ReadDoubleWord 0x580025C4
    ${w6}=    Execute Command    sysbus ReadDoubleWord 0x580025C8
    ${w7}=    Execute Command    sysbus ReadDoubleWord 0x580025CC
    Should Be Equal As Integers    ${w0.strip()}    0x227873d1
    Should Be Equal As Integers    ${w1.strip()}    0x9e04b79d
    Should Be Equal As Integers    ${w2.strip()}    0x3ce98229
    Should Be Equal As Integers    ${w3.strip()}    0xba7dade6
    Should Be Equal As Integers    ${w4.strip()}    0x9f7430db
    Should Be Equal As Integers    ${w5.strip()}    0x293d9ac6
    Should Be Equal As Integers    ${w6.strip()}    0xdb8ed040
    Should Be Equal As Integers    ${w7.strip()}    0x07775510
