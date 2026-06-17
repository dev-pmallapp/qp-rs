*** Settings ***
Documentation     Phase 5 (STS §10.1) regression baseline suite. Three test
...               cases, one per baseline type catalogued in the spec:
...
...               1. **UART line-order** — boots the multi-node txrx
...                  platform and asserts the OHT UART transcript matches
...                  `configs/renode/baselines/txrx_uart.uart-expect.txt`
...                  line-by-line in order. Catches firmware regressions
...                  that drop or reorder boot/telemetry anchors.
...
...               2. **Snapshot save/reload** — saves the OHT machine after
...                  the first post-pairing telemetry, resets the emulation,
...                  reloads the snapshot, and asserts that telemetry
...                  continues. Catches peripheral-stub serialization
...                  regressions (Renode `Save`/`Load` quietly drops state
...                  if a stub doesn't implement it).
...
...               3. **PCAP frame count** — enables
...                  `LogIEEE802_15_4Traffic` on the shared wireless
...                  medium, runs the txrx telemetry round-trip, then
...                  asserts the captured `logs/baseline_lora.pcap` has at
...                  least `lora_traffic.frame_count` frames. Floor-only;
...                  the baseline is intentionally conservative — see
...                  configs/renode/baselines/README.md for the
...                  raise-on-stable-run playbook.
...
...               Build prerequisite (from repo root):
...                   make renode-baselines
...               which builds both ESP32-C6 ELFs with `lr1121,renode`
...               (same as test-renode-txrx — clean UART, no QS
...               interleave) and then invokes this suite.
...
...               Stages share one live emulation (`Test Setup`/`Test
...               Teardown` are `No Operation`), same convention as
...               integration/txrx/txrx.robot. Cases must not be reordered.
Test Setup        No Operation
Test Teardown     No Operation
Resource          ${RENODEKEYWORDS}
Resource          ${CURDIR}/../../shared/robot-keywords/baseline_keywords.robot
Resource          ${CURDIR}/../../shared/robot-keywords/txrx_keywords.robot
Test Timeout      120 seconds

*** Variables ***
${BASELINE_UART_BASE}      ${BASELINE_DIR}${/}txrx_uart.uart-expect.txt
${BASELINE_PCAP_COUNT}     ${BASELINE_DIR}${/}lora_traffic.frame_count
${RESULTS_DIR}             ${BASELINE_PROJECT_ROOT}${/}target${/}test-results${/}regression${/}baselines
${LOGS_DIR}                ${BASELINE_PROJECT_ROOT}${/}logs
${PCAP_PATH}               ${LOGS_DIR}${/}baseline_lora.pcap
${SNAPSHOT_PATH}           ${RESULTS_DIR}${/}snapshot.dat
${WIRELESS_MEDIUM_NAME}    wireless

*** Test Cases ***

Stage 1 — Boot With PCAP Capture Enabled
    [Documentation]    Bring up the multi-node txrx platform and enable
    ...                IEEE 802.15.4 traffic logging to a baseline PCAP
    ...                path. The setup deliberately mirrors txrx Stage 1
    ...                so the rest of the run is identical to a real
    ...                regression check.
    Create Directory    ${LOGS_DIR}
    Remove File         ${PCAP_PATH}
    Create Directory    ${RESULTS_DIR}
    Setup Txrx Platform
    # Capture LoRa frames for the §10.1 PCAP baseline. Enable after the
    # resc loads so $medium_name (renode resc variable, distinct from the
    # Robot ${WIRELESS_MEDIUM_NAME}) has been seeded.
    Execute Command    emulation LogIEEE802_15_4Traffic @${PCAP_PATH} ${WIRELESS_MEDIUM_NAME}
    Wait For Both Nodes Boot

Stage 2 — UART Boot Sequence Matches Baseline
    [Documentation]    Walk the txrx_uart.uart-expect.txt baseline against
    ...                the OHT UART transcript. The first row asserts the
    ...                boot banner that Stage 1 already observed (it's a
    ...                no-op for the tester since the line is already in
    ...                its history); subsequent rows wait on the first
    ...                post-pairing telemetry anchor. A missing row →
    ...                Renode TimeoutError → baseline-drift failure.
    Assert UART Lines Match Baseline    ${BASELINE_UART_BASE}    ${gagan_uart}    timeout=60

Stage 3 — PCAP Frame Count Meets Baseline
    [Documentation]    Quiesce the emulation so Renode flushes the pcap,
    ...                then run tshark (or fall back to size-floor) and
    ...                assert at least ${BASELINE_PCAP_COUNT} frames.
    Execute Command    pause
    Assert PCAP Frame Count Meets Baseline    ${PCAP_PATH}    ${BASELINE_PCAP_COUNT}
    Execute Command    start

Stage 4 — Snapshot Save Reset And Reload Continues Telemetry
    [Documentation]    Snapshot the OHT machine, reset the emulation,
    ...                reload from snapshot, and confirm telemetry
    ...                continues. A failing reload manifests as a
    ...                Wait-For-Line timeout after the reset (state was
    ...                lost) or an emulation halt — either fails the case.
    ...                Snapshot artifact lives under target/test-results/
    ...                regression/baselines/ and is not committed (snapshots
    ...                are gitignored at the repo root).
    Execute Command    mach set "SWM-Gagan-OHT"
    Snapshot Save Reset And Reload    ${SNAPSHOT_PATH}
    # After reload the per-machine tester state is gone — re-create a
    # tester for the OHT and wait for the next telemetry cycle.
    ${reloaded_uart}=    Create Terminal Tester    sysbus.usb_serial_jtag    machine=SWM-Gagan-OHT
    Wait For Line On Uart    SWM TX telemetry    testerId=${reloaded_uart}    timeout=60
    [Teardown]    Reset Emulation
