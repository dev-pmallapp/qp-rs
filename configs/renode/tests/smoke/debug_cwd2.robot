*** Settings ***
Resource          ${RENODEKEYWORDS}

*** Variables ***
${PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..
${RESC}            ${PROJECT_ROOT}${/}configs${/}renode${/}swm${/}swm_multinode_lr1121.resc

*** Test Cases ***
Debug CWD After Include
    [Teardown]    Reset Emulation
    Execute Command    python "import System.IO; print('Before SetCD:', System.IO.Directory.GetCurrentDirectory())"
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${PROJECT_ROOT}')"
    Execute Command    python "import System.IO; print('After SetCD:', System.IO.Directory.GetCurrentDirectory())"
    Execute Command    python "import System.IO; print('RESC path:', '${RESC}')"
