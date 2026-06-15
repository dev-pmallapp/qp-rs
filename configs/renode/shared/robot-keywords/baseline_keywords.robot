*** Settings ***
Documentation     Shared keywords for the regression baseline suite (STS §10.1).
...               The three baseline types each get one helper here:
...               - `Assert UART Lines Match Baseline` reads an expected-lines
...                 file and runs `Wait For Line On Uart` for each row in
...                 order.
...               - `Snapshot Save Reset And Reload` exercises the
...                 `Save`/`Load` Renode commands on the active machine.
...               - `Assert PCAP Frame Count Meets Baseline` reads the
...                 minimum-frame-count baseline file and uses `tshark` to
...                 count frames; gracefully no-ops if tshark is missing.
Library           Collections
Library           OperatingSystem
Library           Process
Library           String
Resource          ${RENODEKEYWORDS}

*** Variables ***
${BASELINE_PROJECT_ROOT}    ${CURDIR}${/}..${/}..${/}..${/}..
${BASELINE_DIR}             ${BASELINE_PROJECT_ROOT}${/}configs${/}renode${/}baselines
${BASELINE_UART_TIMEOUT}    20

*** Keywords ***

Read Baseline Lines
    [Documentation]    Read a `.uart-expect.txt`-style file and return a
    ...                Robot list of non-comment, non-blank rows in order.
    [Arguments]    ${path}
    File Should Exist    ${path}
    ${raw}=    Get File    ${path}
    @{rows}=    Create List
    FOR    ${line}    IN    @{raw.splitlines()}
        ${stripped}=    Strip String    ${line}
        Continue For Loop If    '${stripped}' == ''
        Continue For Loop If    '${stripped[0:1]}' == '#'
        Append To List    ${rows}    ${stripped}
    END
    RETURN    ${rows}

Assert UART Lines Match Baseline
    [Documentation]    Wait for each expected line in the baseline file to
    ...                appear on the given tester id, in order. A missing
    ...                line raises a Renode TimeoutError, which the suite
    ...                will surface as a baseline-drift failure.
    [Arguments]    ${baseline_path}    ${tester}    ${timeout}=${BASELINE_UART_TIMEOUT}
    @{expected}=    Read Baseline Lines    ${baseline_path}
    FOR    ${line}    IN    @{expected}
        Wait For Line On Uart    ${line}    testerId=${tester}    timeout=${timeout}
    END

Snapshot Save Reset And Reload
    [Documentation]    Save the active machine to a temp file, reset the
    ...                emulation, then reload the snapshot. Returns the
    ...                snapshot file path so the caller can clean it up.
    [Arguments]    ${snapshot_path}
    Create Directory    ${snapshot_path.rsplit('${/}', 1)[0]}
    Execute Command    pause
    Execute Command    Save @${snapshot_path}
    Execute Command    Reset Emulation
    Execute Command    Load @${snapshot_path}
    Execute Command    start
    RETURN    ${snapshot_path}

Read Frame Count Baseline
    [Documentation]    Read a single-integer baseline file. Whitespace and
    ...                comment lines (starting with '#') are stripped.
    [Arguments]    ${path}
    File Should Exist    ${path}
    ${raw}=    Get File    ${path}
    FOR    ${line}    IN    @{raw.splitlines()}
        ${stripped}=    Strip String    ${line}
        Continue For Loop If    '${stripped}' == ''
        Continue For Loop If    '${stripped[0:1]}' == '#'
        ${value}=    Convert To Integer    ${stripped}
        RETURN    ${value}
    END
    Fail    Baseline ${path} has no value line.

Assert PCAP Frame Count Meets Baseline
    [Documentation]    Count frames in `${pcap_path}` via tshark and assert
    ...                the count meets or exceeds the baseline at
    ...                `${baseline_path}`. If tshark is not installed the
    ...                keyword falls back to a file-size sanity check (the
    ...                global header is 24 bytes; any captured record adds
    ...                at least 16 bytes), preserving the regression intent
    ...                without forcing CI to ship the wireshark suite.
    [Arguments]    ${pcap_path}    ${baseline_path}
    File Should Exist    ${pcap_path}
    ${minimum}=    Read Frame Count Baseline    ${baseline_path}
    ${rc}=    Run And Return Rc    tshark --version
    IF    ${rc} != 0
        ${size}=    Get File Size    ${pcap_path}
        ${needed}=    Evaluate    24 + (${minimum} * 16)
        Should Be True    ${size} >= ${needed}
        ...    PCAP ${pcap_path} size=${size} below the tshark-less floor (${needed} = 24 + minimum*16); medium logging may be disabled.
        Log    tshark not installed — used size-based floor.
        RETURN
    END
    ${frames}=    Run    tshark -r ${pcap_path} -q -z io,stat,0 2>/dev/null | awk '/Frames:/ {print $2; exit}'
    Should Not Be Empty    ${frames}    msg=tshark produced no Frames: line from ${pcap_path}
    ${count}=    Convert To Integer    ${frames}
    Should Be True    ${count} >= ${minimum}
    ...    PCAP ${pcap_path} parsed ${count} frames, baseline minimum is ${minimum}. Update configs/renode/baselines/lora_traffic.frame_count if intentional.
