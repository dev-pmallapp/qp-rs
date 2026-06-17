*** Settings ***
Documentation     STM32U5_HASH isolation tests — drives the C# HASH model via
...               sysbus reads/writes, no firmware running.  Verifies register
...               decode and SHA-256 output against NIST FIPS-180-4 test
...               vectors (empty input, "abc", and the 56-byte multi-block
...               vector).
...               Phase 5.3 of the STM32 Renode parity plan.
...               Tags: crypto, peripheral, unit, §5.3

Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..${/}..
${RESC_PATH}         ${PROJECT_ROOT}${/}configs${/}renode${/}tests${/}peripherals${/}crypto${/}test_hash_platform.resc

${HASH_BASE}         0x420C0400
${HASH_CR}           0x420C0400
${HASH_DIN}          0x420C0404
${HASH_STR}          0x420C0408
${HASH_HR0}          0x420C040C
${HASH_HR1}          0x420C0410
${HASH_HR2}          0x420C0414
${HASH_HR3}          0x420C0418
${HASH_HR4}          0x420C041C
${HASH_SR}           0x420C0424
${HASH_HR5}          0x420C0710
${HASH_HR6}          0x420C0714
${HASH_HR7}          0x420C0718
${HASH_OP_COUNT}     0x420C07F0
${HASH_FORCE_FAIL}   0x420C07F4

# HASH_CR: ALGO[1:0] mapped to bits 17, 7 (SHA-256 = 11).
# Bit 0: INIT (start a new digest).
${CR_INIT_SHA256}    0x20081
${CR_RUN_SHA256}     0x20080
# HASH_STR: bit 8 = DCAL, bits 0..4 = NBLW (valid bits in last word).
${STR_FINAL_BIT}     0x100
${STR_FINAL_24BIT}   0x118
${STR_FINAL_32BIT}   0x100


*** Test Cases ***

5.3.1 Reset Leaves CR=0 OP_COUNT=0 And DINIS=1
    [Setup]    Setup Platform
    ${cr}=    Execute Command    sysbus ReadDoubleWord ${HASH_CR}
    Should Be Equal As Integers    ${cr.strip()}    0
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${HASH_SR}
    Should Be Equal As Integers    ${sr.strip()}    1    msg=DINIS not set at reset
    ${oc}=    Execute Command    sysbus ReadDoubleWord ${HASH_OP_COUNT}
    Should Be Equal As Integers    ${oc.strip()}    0
    [Teardown]    Reset Emulation

5.3.2 SHA-256 Of Empty Input Matches FIPS-180-4
    [Documentation]    SHA-256("") =
    ...                e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${HASH_CR} ${CR_INIT_SHA256}
    Execute Command    sysbus WriteDoubleWord ${HASH_STR} ${STR_FINAL_BIT}
    Read And Assert SHA256 Digest
    ...    0xe3b0c442    0x98fc1c14    0x9afbf4c8    0x996fb924
    ...    0x27ae41e4    0x649b934c    0xa495991b    0x7852b855
    ${oc}=    Execute Command    sysbus ReadDoubleWord ${HASH_OP_COUNT}
    Should Be Equal As Integers    ${oc.strip()}    1
    [Teardown]    Reset Emulation

5.3.3 SHA-256 Of "abc" Matches FIPS-180-4
    [Documentation]    SHA-256("abc") =
    ...                ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${HASH_CR} ${CR_INIT_SHA256}
    # "abc" packed as one big-endian word, trailing byte will be dropped via NBLW=24.
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x61626300
    Execute Command    sysbus WriteDoubleWord ${HASH_STR} ${STR_FINAL_24BIT}
    Read And Assert SHA256 Digest
    ...    0xba7816bf    0x8f01cfea    0x414140de    0x5dae2223
    ...    0xb00361a3    0x96177a9c    0xb410ff61    0xf20015ad
    [Teardown]    Reset Emulation

5.3.4 SHA-256 Of Two-Block Vector Matches FIPS-180-4
    [Documentation]    SHA-256("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq") =
    ...                248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${HASH_CR} ${CR_INIT_SHA256}
    Feed 56 Byte Vector
    Execute Command    sysbus WriteDoubleWord ${HASH_STR} ${STR_FINAL_32BIT}
    Read And Assert SHA256 Digest
    ...    0x248d6a61    0xd20638b8    0xe5c02693    0x0c3e6039
    ...    0xa33ce459    0x64ff2167    0xf6ecedd4    0x19db06c1
    [Teardown]    Reset Emulation

5.3.5 INIT Bit Discards Buffered Bytes
    [Documentation]    Write a few DIN words, hit INIT again, hash empty;
    ...                the empty-digest must match the FIPS empty-input vector.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${HASH_CR} ${CR_INIT_SHA256}
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0xDEADBEEF
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0xCAFEBABE
    Execute Command    sysbus WriteDoubleWord ${HASH_CR} ${CR_INIT_SHA256}
    Execute Command    sysbus WriteDoubleWord ${HASH_STR} ${STR_FINAL_BIT}
    Read And Assert SHA256 Digest
    ...    0xe3b0c442    0x98fc1c14    0x9afbf4c8    0x996fb924
    ...    0x27ae41e4    0x649b934c    0xa495991b    0x7852b855
    [Teardown]    Reset Emulation

5.3.6 FORCE_FAIL Returns Zeroed HR Block And Latches DCIS
    [Documentation]    Robot test surface for digest-error paths.
    [Setup]    Setup Platform
    Execute Command    sysbus WriteDoubleWord ${HASH_FORCE_FAIL} 1
    Execute Command    sysbus WriteDoubleWord ${HASH_CR} ${CR_INIT_SHA256}
    Execute Command    sysbus WriteDoubleWord ${HASH_STR} ${STR_FINAL_BIT}
    Read And Assert SHA256 Digest    0    0    0    0    0    0    0    0
    ${sr}=    Execute Command    sysbus ReadDoubleWord ${HASH_SR}
    ${v}=    Convert To Integer    ${sr.strip()}
    ${dcis}=    Evaluate    (${v} >> 1) & 1
    Should Be Equal As Integers    ${dcis}    1
    [Teardown]    Reset Emulation


*** Keywords ***

Setup Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    logLevel 3

Feed 56 Byte Vector
    # "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x61626364
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x62636465
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x63646566
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x64656667
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x65666768
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x66676869
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x6768696A
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x68696A6B
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x696A6B6C
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x6A6B6C6D
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x6B6C6D6E
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x6C6D6E6F
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x6D6E6F70
    Execute Command    sysbus WriteDoubleWord ${HASH_DIN} 0x6E6F7071

Read And Assert SHA256 Digest
    [Arguments]    ${w0}    ${w1}    ${w2}    ${w3}    ${w4}    ${w5}    ${w6}    ${w7}
    ${r0}=    Execute Command    sysbus ReadDoubleWord ${HASH_HR0}
    ${r1}=    Execute Command    sysbus ReadDoubleWord ${HASH_HR1}
    ${r2}=    Execute Command    sysbus ReadDoubleWord ${HASH_HR2}
    ${r3}=    Execute Command    sysbus ReadDoubleWord ${HASH_HR3}
    ${r4}=    Execute Command    sysbus ReadDoubleWord ${HASH_HR4}
    ${r5}=    Execute Command    sysbus ReadDoubleWord ${HASH_HR5}
    ${r6}=    Execute Command    sysbus ReadDoubleWord ${HASH_HR6}
    ${r7}=    Execute Command    sysbus ReadDoubleWord ${HASH_HR7}
    Should Be Equal As Integers    ${r0.strip()}    ${w0}
    Should Be Equal As Integers    ${r1.strip()}    ${w1}
    Should Be Equal As Integers    ${r2.strip()}    ${w2}
    Should Be Equal As Integers    ${r3.strip()}    ${w3}
    Should Be Equal As Integers    ${r4.strip()}    ${w4}
    Should Be Equal As Integers    ${r5.strip()}    ${w5}
    Should Be Equal As Integers    ${r6.strip()}    ${w6}
    Should Be Equal As Integers    ${r7.strip()}    ${w7}
