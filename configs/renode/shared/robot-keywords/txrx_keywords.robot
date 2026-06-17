*** Settings ***
Documentation     Shared keywords for the staged TX/RX integration flow (Demo 1).
...               Setup Txrx Platform seeds the chain; each milestone keyword
...               asserts one stage's UART output. UART testers are global so they
...               survive across the ordered stage test cases. Like the FOTA flow,
...               the wireless stack does not serialize, so the stages share one
...               live emulation rather than a snapshot Provides/Requires chain.
Library           Collections
Resource          ${RENODEKEYWORDS}

*** Variables ***
${TXRX_PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..
${TXRX_RESC}            ${TXRX_PROJECT_ROOT}${/}configs${/}renode${/}swm${/}swm_multinode_lr1121.resc

*** Keywords ***

Setup Txrx Platform
    [Documentation]    Stage 1 setup: load the multi-node platform, silence verbose
    ...                logging, create the two per-node UART testers as suite
    ...                variables, then start one continuously-running emulation that
    ...                every stage test case asserts against in turn.
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${TXRX_PROJECT_ROOT}')"
    Execute Command    include @${TXRX_RESC}
    Execute Command    mach set "SWM-Gagan-OHT"
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    mach set "SWM-Pramukh-MC"
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    ${gagan_uart}=     Create Terminal Tester    sysbus.usb_serial_jtag    machine=SWM-Gagan-OHT
    ${pramukh_uart}=   Create Terminal Tester    sysbus.usb_serial_jtag    machine=SWM-Pramukh-MC
    Set Suite Variable    ${gagan_uart}
    Set Suite Variable    ${pramukh_uart}
    Start Emulation

Wait For Both Nodes Boot
    [Documentation]    Stage 1 milestone — both nodes print "SWM boot".
    Wait For Line On Uart    SWM boot    testerId=${gagan_uart}      timeout=10
    Wait For Line On Uart    SWM boot    testerId=${pramukh_uart}    timeout=10

Wait For Telemetry Round Trip
    [Documentation]    Stage 2 milestone — OHT samples its level sensor and transmits
    ...                a SWM telemetry frame; MC receives and decodes it over the LoRa
    ...                wireless medium.
    Wait For Line On Uart    SWM TX telemetry    testerId=${gagan_uart}      timeout=60
    Wait For Line On Uart    SWM RX telemetry    testerId=${pramukh_uart}    timeout=30
