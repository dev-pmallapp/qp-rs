*** Settings ***
Documentation     Demo 2 — pairing + version-gated FOTA over LoRa, decomposed into
...               ordered stages. Stage 1 boots one emulation that then runs
...               continuously; each later stage asserts the next milestone in the
...               firmware's Active-Object progression, so a failing stage pinpoints
...               exactly where the flow breaks. FOTA application is the final stage.
...
...               Build prerequisite (from repo root):
...                 make test-renode-fota
...               which builds both ESP32-C6 ELFs with the `fota-demo` cargo feature
...               (OHT fw_ver=1 < MC target fw_ver=2) and runs this suite.
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
Resource          ${CURDIR}/../../../shared/robot-keywords/fota_keywords.robot
# Wall-clock budget per stage. Wait-for-line timeouts are in *virtual* seconds and
# this crypto/radio workload simulates well below real time, so each stage needs a
# generous wall-clock ceiling (the original monolith used 600s for the whole flow).
Test Timeout      600 seconds

*** Test Cases ***

Stage 1 — Both Nodes Boot
    [Documentation]    Bring up the platform and start the shared emulation; both
    ...                nodes reach the SWM boot banner.
    Setup Fota Platform
    Wait For Both Nodes Boot

Stage 2 — Nodes Complete Pairing
    [Documentation]    PairRequest → PairAck; OHT learns the MC target firmware
    ...                version (AO pairing state transitions).
    Wait For Pairing Complete

Stage 3 — OHT Receives Fota Manifest
    [Documentation]    Version gate opens (OHT fw_ver < target); OHT receives the
    ...                manifest and enters the updating state.
    Wait For Fota Manifest

Stage 4 — OHT Accepts All Chunks
    [Documentation]    All four encrypted ChaCha20 chunks are accepted in order.
    Wait For All Chunks

Stage 5 — Fota Completes
    [Documentation]    Candidate image CRC-verified and applied — FOTA complete.
    Wait For Fota Complete
    [Teardown]    Reset Emulation
