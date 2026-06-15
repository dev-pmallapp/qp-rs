*** Settings ***
Resource    ${RENODEKEYWORDS}

*** Test Cases ***
Renode Responds To Basic Command
    ${v}=    Execute Command    version
    Log    ${v}
    Should Contain    ${v}    Renode
