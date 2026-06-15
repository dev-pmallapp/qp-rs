*** Settings ***
Documentation     Performance regression tests — measures platform load time and
...               virtual-to-real simulation time ratio.
...               Covers TestingTopics.md §10.1 (startup time) and §10.2 (sim time ratio).
...               Tags: performance, timing, §10.1, §10.2

Resource          ${RENODEKEYWORDS}
Library           Process
Library           String

*** Variables ***
${PROJECT_ROOT}         ${CURDIR}${/}..${/}..${/}..${/}..
${RESC_PATH}            ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc
${LOAD_TIME_LIMIT_S}    30
${SIM_RATIO_MIN}        0.005

*** Test Cases ***

10.1 Platform Loads Within Time Budget
    [Documentation]    Measures wall-clock seconds to load the ESP32-C6 LR1121 platform
    ...                (include + ROM ELF + firmware ELF).  Must complete within
    ...                ${LOAD_TIME_LIMIT_S} seconds.  §10.1
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    ${t0}=    Evaluate    __import__('time').time()
    Execute Command    include @${RESC_PATH}
    ${t1}=    Evaluate    __import__('time').time()
    ${elapsed}=    Evaluate    round(${t1} - ${t0}, 2)
    Log    Platform load time: ${elapsed} s
    Should Be True    ${elapsed} < ${LOAD_TIME_LIMIT_S}
    ...    msg=Platform load took ${elapsed}s, exceeds ${LOAD_TIME_LIMIT_S}s budget
    [Teardown]    Reset Emulation

10.2 Simulation Time Ratio Above Minimum
    [Documentation]    Runs the firmware for 1 virtual second and measures real elapsed
    ...                time.  Virtual-to-real ratio must be >= ${SIM_RATIO_MIN}.
    ...                A ratio below this indicates Renode or peripheral-stub performance
    ...                regression.  §10.2
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    logLevel 0
    ${t0}=    Evaluate    __import__('time').time()
    Execute Command    emulation RunFor "00:00:01"
    ${t1}=    Evaluate    __import__('time').time()
    ${real_s}=    Evaluate    round(${t1} - ${t0}, 3)
    ${ratio}=    Evaluate    round(1.0 / max(${real_s}, 0.001), 3)
    Log    RunFor 1 virtual second took ${real_s} s real time (ratio=${ratio}x)
    Should Be True    ${ratio} >= ${SIM_RATIO_MIN}
    ...    msg=Sim ratio ${ratio}x is below minimum ${SIM_RATIO_MIN}x
    [Teardown]    Reset Emulation
