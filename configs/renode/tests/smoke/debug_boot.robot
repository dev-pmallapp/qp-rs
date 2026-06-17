*** Settings ***
Documentation     Diagnostic — checks if Gagan firmware boots and emits to USB JTAG.
Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..
${RESC}            ${PROJECT_ROOT}${/}configs${/}renode${/}swm${/}swm_multinode_lr1121.resc

*** Test Cases ***

Gagan Emits SWM Boot Within 3 Virtual Seconds
    [Teardown]    Reset Emulation
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    logLevel 3
    ${uart}=    Create Terminal Tester    sysbus.usb_serial_jtag    machine=SWM-Gagan-OHT
    Wait For Line On Uart    SWM boot    testerId=${uart}    timeout=3
