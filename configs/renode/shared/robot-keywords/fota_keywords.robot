*** Settings ***
Documentation     Shared keywords for the staged FOTA integration flow (Demo 2).
...               Setup Fota Platform seeds the Provides/Requires chain; each
...               milestone keyword asserts one stage's UART output. UART testers
...               are global so they survive snapshot restore across stage files.
Library           Collections
Resource          ${RENODEKEYWORDS}

*** Variables ***
${FOTA_PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..
${FOTA_RESC}            ${FOTA_PROJECT_ROOT}${/}configs${/}renode${/}swm${/}swm_multinode_lr1121.resc

*** Keywords ***

Setup Fota Platform
    [Documentation]    Suite setup: load the multi-node platform, silence verbose
    ...                logging, create the two per-node UART testers as suite
    ...                variables, then start one continuously-running emulation that
    ...                every stage test case asserts against in turn.
    ...
    ...                The wireless stack (LR1121Radio + IEEE 802.15.4 medium) does
    ...                not serialize, so the Renode snapshot Provides/Requires chain
    ...                cannot be used here; the stages instead share one live
    ...                emulation and progress through the firmware's AO transitions.
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${FOTA_PROJECT_ROOT}')"
    Execute Command    include @${FOTA_RESC}
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

Wait For Pairing Complete
    [Documentation]    Stage 2 milestone — PairRequest through PairAck; OHT learns
    ...                the MC target firmware version (target_ver=2). The HMAC
    ...                challenge/response is performed internally and not logged, so
    ...                only the request and the resulting PairAck lines are asserted.
    Wait For Line On Uart    SWM PAIR request              testerId=${gagan_uart}      timeout=30
    Wait For Line On Uart    SWM PAIR request from=2       testerId=${pramukh_uart}    timeout=15
    Wait For Line On Uart    SWM PAIR ok node=2            testerId=${pramukh_uart}    timeout=30
    Wait For Line On Uart    SWM PAIR ok target_ver=2      testerId=${gagan_uart}      timeout=15

Wait For Fota Manifest
    [Documentation]    Stage 3 milestone — version-gated FOTA opens; OHT receives
    ...                the manifest (hw=1 ver=2 size=512).
    Wait For Line On Uart    SWM FOTA start dest=2                  testerId=${pramukh_uart}    timeout=15
    Wait For Line On Uart    SWM FOTA manifest hw=1 ver=2 size=512  testerId=${gagan_uart}      timeout=30

Wait For All Chunks
    [Documentation]    Stage 4 milestone — all four encrypted chunks accepted.
    Wait For Line On Uart    SWM FOTA chunk 0 ok    testerId=${gagan_uart}    timeout=30
    Wait For Line On Uart    SWM FOTA chunk 1 ok    testerId=${gagan_uart}    timeout=15
    Wait For Line On Uart    SWM FOTA chunk 2 ok    testerId=${gagan_uart}    timeout=15
    Wait For Line On Uart    SWM FOTA chunk 3 ok    testerId=${gagan_uart}    timeout=15

Wait For Fota Complete
    [Documentation]    Stage 5 milestone — candidate image verified and applied.
    Wait For Line On Uart    SWM FOTA verified crc=ok    testerId=${gagan_uart}      timeout=15
    Wait For Line On Uart    SWM FOTA complete           testerId=${pramukh_uart}    timeout=15
