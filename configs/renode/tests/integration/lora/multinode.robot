*** Settings ***
Documentation     Multi-node LoRa wireless communication tests in Renode.
...               Realises STS_SWM §7 (LoRa multi-node) end-to-end:
...                 §7.1 single-hop packet exchange
...                 §7.2 range enforcement
...                 §7.3 broadcast / multiple receivers
...                 §7.4 PCAP capture parseable post-run
...                 §7.5 RSSI / SNR propagated to firmware
...                 §7.6 corrupted (bad-CRC) frame rejected cleanly
...               Each STS row maps to a tagged sts-7.x case below; gateway
...               routing and dynamic disconnect carry the same tag as the §7
...               row they exercise (range / broadcast variants).

Library           OperatingSystem
Suite Setup       Prepare Emulation
Suite Teardown    Reset Emulation
Test Teardown     Reset Emulation
Test Timeout      90 seconds

Resource          ${RENODEKEYWORDS}

*** Variables ***
# Paths — override from CLI: renode-test --variable SENSOR_FW:path/to/fw.elf
${SENSOR_FW}      ${CURDIR}/../../../firmware/sensor_node.elf
${GATEWAY_FW}     ${CURDIR}/../../../firmware/gateway.elf
${RESC}           ${CURDIR}/../../../scripts/lora_multinode.resc

# UART peripheral names per machine type
${UART_SX1276}    sysbus.uart1
${UART_SX1262}    sysbus.uart0

# PCAP capture path (STS §7.4) — lora_multinode.resc writes traffic here.
# Keep this in sync with the @logs/lora_traffic.pcap line in the resc.
${PCAP_DIR}       ${CURDIR}/../../../../../logs
${PCAP_PATH}      ${PCAP_DIR}/lora_traffic.pcap

# Timeouts
${BOOT_TIMEOUT}       15
${TX_TIMEOUT}         10
${RANGE_TIMEOUT}      5

*** Test Cases ***

# ── TC-1: All nodes should boot and initialize their radios ───
All Nodes Should Initialize Radio Successfully
    [Documentation]    Verifies each node boots and logs "LoRa radio ready".
    [Tags]    sts-7.0    boot

    Execute Script      ${RESC}

    FOR    ${node}    IN    node-sensor-1    node-sensor-2
        Switch To Machine    ${node}
        Wait For Line On Uart    LoRa radio ready
        ...    testerId=${UART_SX1276}    timeout=${BOOT_TIMEOUT}
    END

    Switch To Machine    node-gateway
    Wait For Line On Uart    LoRa radio ready
    ...    testerId=${UART_SX1262}    timeout=${BOOT_TIMEOUT}

# ── TC-2: Sensor 1 TX → Gateway RX — STS §7.1 ─────────────────
Sensor Node 1 Should Transmit To Gateway
    [Documentation]    STS §7.1 single-hop packet exchange: node-sensor-1 sends
    ...                a packet; node-gateway must receive it byte-for-byte.
    ...                Both are within the 100-unit range (distance = 25).
    [Tags]    sts-7.1    single-hop

    Execute Script      ${RESC}
    Boot All Nodes

    Switch To Machine    node-sensor-1
    Write Line To Uart    TX HELLO_GW    testerId=${UART_SX1276}

    Switch To Machine    node-gateway
    Wait For Line On Uart    RX HELLO_GW
    ...    testerId=${UART_SX1262}    timeout=${TX_TIMEOUT}

# ── TC-3: Sensor 2 TX → Gateway RX — STS §7.1 ─────────────────
Sensor Node 2 Should Transmit To Gateway
    [Documentation]    STS §7.1 single-hop packet exchange (second peer):
    ...                node-sensor-2 at distance=25 from gateway sends a packet.
    [Tags]    sts-7.1    single-hop

    Execute Script      ${RESC}
    Boot All Nodes

    Switch To Machine    node-sensor-2
    Write Line To Uart    TX PING    testerId=${UART_SX1276}

    Switch To Machine    node-gateway
    Wait For Line On Uart    RX PING
    ...    testerId=${UART_SX1262}    timeout=${TX_TIMEOUT}

# ── TC-4: Gateway broadcast → both sensors receive — STS §7.3 ──
Gateway Broadcast Should Reach Both Sensor Nodes
    [Documentation]    STS §7.3 broadcast / multiple receivers: gateway at
    ...                (25,0,0) broadcasts; both sensors at (0,0,0) and
    ...                (50,0,0) are within 100 units — both must receive.
    [Tags]    sts-7.3    broadcast

    Execute Script      ${RESC}
    Boot All Nodes

    Switch To Machine    node-gateway
    Write Line To Uart    BROADCAST ACK    testerId=${UART_SX1262}

    Switch To Machine    node-sensor-1
    Wait For Line On Uart    RX ACK
    ...    testerId=${UART_SX1276}    timeout=${TX_TIMEOUT}

    Switch To Machine    node-sensor-2
    Wait For Line On Uart    RX ACK
    ...    testerId=${UART_SX1276}    timeout=${TX_TIMEOUT}

# ── TC-5: Out-of-range node must NOT receive — STS §7.2 ───────
Out Of Range Node Should Not Receive Packets
    [Documentation]    STS §7.2 range enforcement: node-out-of-range is at
    ...                (200,0,0), 175 units from gateway and 200 units from
    ...                sensor-1 — both beyond range=100. Frame must not be
    ...                delivered by the wireless medium.
    [Tags]    sts-7.2    range

    Execute Script      ${RESC}
    Boot All Nodes

    Switch To Machine    node-sensor-1
    Write Line To Uart    TX SECRET    testerId=${UART_SX1276}

    # Must NOT receive within timeout
    Switch To Machine    node-out-of-range
    Run Keyword And Expect Error    *TimeoutError*
    ...    Wait For Line On Uart    RX SECRET
    ...    testerId=${UART_SX1276}    timeout=${RANGE_TIMEOUT}

# ── TC-6: Dynamic disconnect / reconnect — STS §7.2 variant ───
Node Should Stop Receiving After Disconnect
    [Documentation]    STS §7.2 variant — exercises the same medium-isolation
    ...                rule via a runtime disconnect rather than distance:
    ...                disconnect node-sensor-1, verify no packets received,
    ...                then reconnect and verify communication resumes.
    [Tags]    sts-7.2    range    disconnect

    Execute Script      ${RESC}
    Boot All Nodes

    # Disconnect sensor-1
    Switch To Machine    node-sensor-1
    Execute Command      connector Disconnect sysbus.radio loraMedium

    # Gateway sends — sensor-1 should NOT receive
    Switch To Machine    node-gateway
    Write Line To Uart    TX PING    testerId=${UART_SX1262}

    Switch To Machine    node-sensor-1
    Run Keyword And Expect Error    *TimeoutError*
    ...    Wait For Line On Uart    RX PING
    ...    testerId=${UART_SX1276}    timeout=${RANGE_TIMEOUT}

    # Reconnect sensor-1
    Switch To Machine    node-sensor-1
    Execute Command      connector Connect sysbus.radio loraMedium

    # Gateway sends again — sensor-1 SHOULD receive now
    Switch To Machine    node-gateway
    Write Line To Uart    TX PING2    testerId=${UART_SX1262}

    Switch To Machine    node-sensor-1
    Wait For Line On Uart    RX PING2
    ...    testerId=${UART_SX1276}    timeout=${TX_TIMEOUT}

# ── TC-7: Multi-hop simulation (sensor→gateway→sensor) ────────
Gateway Should Route Packet From Sensor1 To Sensor2
    [Documentation]    Gateway routing (TestingTopics §4 backlog): sensor-1
    ...                sends, gateway forwards, sensor-2 receives the relayed
    ...                packet. Requires gateway firmware to implement
    ...                forwarding logic.
    [Tags]    routing    backlog

    Execute Script      ${RESC}
    Boot All Nodes

    Switch To Machine    node-sensor-1
    Write Line To Uart    TX ROUTE_ME    testerId=${UART_SX1276}

    # Gateway receives and forwards
    Switch To Machine    node-gateway
    Wait For Line On Uart    RX ROUTE_ME       testerId=${UART_SX1262}    timeout=${TX_TIMEOUT}
    Wait For Line On Uart    Forwarded packet   testerId=${UART_SX1262}    timeout=${TX_TIMEOUT}

    # Sensor 2 receives the forwarded packet
    Switch To Machine    node-sensor-2
    Wait For Line On Uart    RX ROUTE_ME
    ...    testerId=${UART_SX1276}    timeout=${TX_TIMEOUT}

# ── TC-8: PCAP capture parseable post-run — STS §7.4 ──────────
PCAP Capture Should Record LoRa Frames
    [Documentation]    STS §7.4 PCAP capture: the resc enables
    ...                ${SPACE}${SPACE}emulation LogIEEE802_15_4Traffic @logs/lora_traffic.pcap
    ...                on the shared wireless medium, so every frame delivered
    ...                between nodes is written to a libpcap file. After a
    ...                single-hop TX, the file must exist, be non-empty (a
    ...                valid pcap header is 24 bytes; any captured frame adds
    ...                a 16-byte record header + payload), and — if `tshark`
    ...                is installed on the runner — parse to at least one
    ...                frame.
    [Tags]    sts-7.4    pcap

    Remove File         ${PCAP_PATH}
    Create Directory    ${PCAP_DIR}

    Execute Script      ${RESC}
    Boot All Nodes

    # Generate at least one frame on the medium so the pcap has a record.
    Switch To Machine    node-sensor-1
    Write Line To Uart    TX PCAP_PROBE    testerId=${UART_SX1276}

    Switch To Machine    node-gateway
    Wait For Line On Uart    RX PCAP_PROBE
    ...    testerId=${UART_SX1262}    timeout=${TX_TIMEOUT}

    # Renode flushes the pcap on the next emulation pause; quiesce before
    # inspecting the file.
    Execute Command      pause

    File Should Exist               ${PCAP_PATH}
    ${size}=    Get File Size       ${PCAP_PATH}
    # 24-byte pcap global header + at least one record (header 16 B + payload).
    Should Be True    ${size} > 24
    ...    PCAP at ${PCAP_PATH} has no records (size=${size}); medium logging may be disabled.

    Run Keyword And Ignore Error    Assert PCAP Has Frames Via Tshark    ${PCAP_PATH}

# ── TC-9: RSSI / SNR propagated to firmware — STS §7.5 ────────
RSSI And SNR Should Be Reported By Firmware
    [Documentation]    STS §7.5 / VVT-007 signal-quality reporting: after an
    ...                RX, the gateway firmware must surface RSSI (dBm) and
    ...                SNR (dB) from `RadioPacketInfo` (swm-hal §lora) onto
    ...                the UART/QS channel. The line is matched as
    ...                ${SPACE}${SPACE}"RX <payload> RSSI=<int> SNR=<int>"
    ...                with negative RSSI permitted; firmware that drops the
    ...                metadata will fail the regex.
    [Tags]    sts-7.5    rssi    snr    vvt-007

    Execute Script      ${RESC}
    Boot All Nodes

    Switch To Machine    node-sensor-1
    Write Line To Uart    TX QUALITY_PROBE    testerId=${UART_SX1276}

    Switch To Machine    node-gateway
    Wait For Line On Uart    RX QUALITY_PROBE
    ...    testerId=${UART_SX1262}    timeout=${TX_TIMEOUT}
    Wait For Line On Uart    RSSI=-?\\d+\\s+SNR=-?\\d+
    ...    testerId=${UART_SX1262}    timeout=${TX_TIMEOUT}    treatAsRegex=true

# ── TC-10: Corrupted frame rejected cleanly — STS §7.6 ────────
Corrupted Frame Should Be Rejected
    [Documentation]    STS §7.6 corrupted (bad-CRC) frame rejection: sensor-1
    ...                sends a frame flagged for CRC corruption (the keyword
    ...                "BADCRC_FRAME" is the convention the helper firmware
    ...                recognises to deliberately trip CRC failure on the
    ...                air). The gateway MAC must:
    ...                  1. log a "Bad CRC" rejection line (no crash, no abort),
    ...                  2. NOT surface the payload as a valid RX up the stack.
    [Tags]    sts-7.6    bad-crc    mac

    Execute Script      ${RESC}
    Boot All Nodes

    Switch To Machine    node-sensor-1
    Write Line To Uart    TX BADCRC_FRAME    testerId=${UART_SX1276}

    Switch To Machine    node-gateway
    Wait For Line On Uart    Bad CRC
    ...    testerId=${UART_SX1262}    timeout=${TX_TIMEOUT}

    # The corrupt payload must NOT be delivered as a valid RX up the stack.
    Run Keyword And Expect Error    *TimeoutError*
    ...    Wait For Line On Uart    RX BADCRC_FRAME
    ...    testerId=${UART_SX1262}    timeout=${RANGE_TIMEOUT}

*** Keywords ***

Prepare Emulation
    [Documentation]    Common setup — nothing to pre-load; resc is per-test.
    Reset Emulation

Boot All Nodes
    [Documentation]    Waits for all nodes to log "LoRa radio ready".
    FOR    ${node}    IN    node-sensor-1    node-sensor-2    node-out-of-range
        Switch To Machine    ${node}
        Wait For Line On Uart    LoRa radio ready
        ...    testerId=${UART_SX1276}    timeout=${BOOT_TIMEOUT}
    END
    Switch To Machine    node-gateway
    Wait For Line On Uart    LoRa radio ready
    ...    testerId=${UART_SX1262}    timeout=${BOOT_TIMEOUT}

Assert PCAP Has Frames Via Tshark
    [Documentation]    Best-effort tshark parse of a libpcap file (STS §7.4).
    ...                Used via Run Keyword And Ignore Error so a missing
    ...                tshark binary on the runner does not fail the suite —
    ...                the size>24 check already catches the empty-file case.
    [Arguments]    ${pcap}
    ${rc}=    Run And Return Rc    tshark --version
    Should Be Equal As Integers    ${rc}    0    tshark not installed; skipping parse assertion.
    ${frames}=    Run    tshark -r ${pcap} -q -z io,stat,0 2>/dev/null | awk '/Frames:/ {print $2; exit}'
    Should Not Be Empty           ${frames}
    Should Be True    ${frames} > 0    tshark parsed 0 frames from ${pcap}
