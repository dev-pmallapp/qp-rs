*** Settings ***
Resource    ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..
${RESC_PATH}       ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc

*** Keywords ***
Load Platform
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}

*** Test Cases ***
First Load
    Load Platform
    ${v}=    Execute Command    cpu0 PC
    Should Not Be Empty    ${v}
    [Teardown]    Run Keyword And Ignore Error    Execute Command    mach clear

Second Load After Teardown
    Load Platform
    ${v}=    Execute Command    cpu0 PC
    Should Not Be Empty    ${v}
    [Teardown]    Run Keyword And Ignore Error    Execute Command    mach clear
