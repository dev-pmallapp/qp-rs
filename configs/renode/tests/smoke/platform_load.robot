*** Settings ***
Resource    ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..
${RESC_PATH}       ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc

*** Test Cases ***
Platform Loads Without Error
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    include @${RESC_PATH}
    # cpu0 is accessible → machine loaded correctly
    ${v}=    Execute Command    cpu0 PC
    Log    PC after load: ${v}
    Should Not Be Empty    ${v}
    [Teardown]    Run Keyword And Ignore Error    Execute Command    mach clear
