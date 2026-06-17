*** Settings ***
Documentation     Shared keywords for the on-target security self-test (STS §9).
...               Setup Security Platform boots a single Gagan node whose firmware
...               (built with the sec-demo feature) runs the swm-protocol::security
...               replay/auth/expiry vectors at boot and prints one SWM SEC line per
...               §9 row. Each milestone keyword asserts one of those lines.
...
...               The lines are emitted in a fixed order during one boot, so the
...               suite's test cases must assert them in that same order against the
...               single shared emulation.
Library           Collections
Resource          ${RENODEKEYWORDS}

*** Variables ***
${SEC_PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..
${SEC_RESC}            ${SEC_PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc

*** Keywords ***

Setup Security Platform
    [Documentation]    Suite setup: load the single-node LR1121 platform, silence
    ...                verbose logging (it drives sim speed to <<1x), create the
    ...                Gagan UART tester as a suite variable, and start one
    ...                continuously-running emulation that every stage asserts against.
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${SEC_PROJECT_ROOT}')"
    Execute Command    include @${SEC_RESC}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    logLevel 0
    ${gagan_uart}=     Create Terminal Tester    sysbus.usb_serial_jtag    machine=ESP32-C6-DevKit-LR1121
    Set Suite Variable    ${gagan_uart}
    Start Emulation

Wait For Boot
    [Documentation]    Stage 1 milestone — the node prints the SWM boot banner.
    Wait For Line On Uart    SWM boot    testerId=${gagan_uart}    timeout=10

Wait For Command Accepted
    [Documentation]    Baseline — a fresh, authentic command is accepted.
    Wait For Line On Uart    SWM SEC command ok    testerId=${gagan_uart}    timeout=10

Wait For Replay Rejected
    [Documentation]    9.1 — a frame-counter regression (replay) is rejected.
    Wait For Line On Uart    SWM SEC replay rejected    testerId=${gagan_uart}    timeout=10

Wait For Forged Source Rejected
    [Documentation]    9.2 — a command signed with the wrong key (forged source)
    ...                fails authentication.
    Wait For Line On Uart    SWM SEC forged rejected    testerId=${gagan_uart}    timeout=10

Wait For Invalid Join Rejected
    [Documentation]    9.3 — a join HMAC computed under the wrong key is rejected.
    Wait For Line On Uart    SWM SEC join rejected    testerId=${gagan_uart}    timeout=10

Wait For Unauthorized Command Rejected
    [Documentation]    9.4 — a tampered (unauthorized) command fails authentication.
    Wait For Line On Uart    SWM SEC unauth rejected    testerId=${gagan_uart}    timeout=10

Wait For Expired Command Rejected
    [Documentation]    9.5 (VVT-004) — a command past its expiry is rejected.
    Wait For Line On Uart    SWM SEC expired rejected    testerId=${gagan_uart}    timeout=10

Wait For Selftest Complete
    [Documentation]    The self-test ran every vector to completion (no FAIL line
    ...                preempted it — a FAIL would have failed an earlier stage).
    Wait For Line On Uart    SWM SEC selftest done    testerId=${gagan_uart}    timeout=10
