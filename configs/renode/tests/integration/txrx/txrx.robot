*** Settings ***
Documentation     Demo 1 — OHT → MC telemetry round trip, decomposed into ordered
...               stages. Stage 1 boots one emulation that then runs continuously;
...               stage 2 asserts the telemetry round trip, so a failing stage
...               pinpoints exactly where the flow breaks.
...
...               No FOTA is initiated in Demo 1 — see integration/fota/fota.robot
...               (Demo 2) for the pairing + FOTA flow.
...
...               Build prerequisite (from repo root):
...                 make test-renode-txrx
...               which builds both ESP32-C6 ELFs with the `lr1121,renode` cargo
...               features and runs this suite.
...
...               Mechanism note: the wireless stack (LR1121Radio + IEEE 802.15.4
...               medium) does not serialize, so Renode's snapshot Provides/Requires
...               staging is unusable here. Instead the stages share ONE live
...               emulation: Test Setup/Teardown are No Operation so the emulation is
...               not reset between cases, and Robot runs cases in file order. The
...               stages below are ordered and must not be reordered.
Test Setup        No Operation
Test Teardown     No Operation
Resource          ${RENODEKEYWORDS}
Resource          ${CURDIR}/../../../shared/robot-keywords/txrx_keywords.robot
Test Timeout      90 seconds

*** Test Cases ***

Stage 1 — Both Nodes Boot
    [Documentation]    Bring up the platform and start the shared emulation; both
    ...                nodes reach the SWM boot banner.
    Setup Txrx Platform
    Wait For Both Nodes Boot

Stage 2 — Telemetry Round Trip
    [Documentation]    OHT samples its level sensor and transmits a SWM telemetry
    ...                frame; MC receives and decodes it.
    Wait For Telemetry Round Trip
    [Teardown]    Reset Emulation
