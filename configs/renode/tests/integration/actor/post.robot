*** Settings ***
Documentation     STS §6.1 — on-target Boot + POST sequence assertion.
...
...               Phase 3 of docs/05-verification/TestPlan_SWM.md: brings the
...               §6 Renode column up to the host column. The OHT firmware
...               prints a fixed sequence of `SWM …` POST lines as each HAL
...               subsystem comes up — this suite asserts they appear in
...               order, no abort fires, and the QS sink emits at least one
...               frame (proving the FSM has made transitions).
...
...               Single-node platform: pairing will time out (no MC peer),
...               which is expected — POST completes before pairing starts,
...               so the assertions land first. The pairing timeout path is
...               covered separately by integration/txrx/txrx.robot.
...
...               Build prerequisite (from repo root):
...                   make renode-actor-post
...               which builds the OHT with `qs,lr1121` and runs this suite.
Test Setup        No Operation
Test Teardown     No Operation
Resource          ${RENODEKEYWORDS}
Resource          ${CURDIR}/../../../shared/robot-keywords/actor_keywords.robot
Test Timeout      60 seconds

*** Test Cases ***

Stage 1 — Boot Banner
    [Documentation]    The very first SWM line printed at main() entry.
    Setup Actor Platform Single Node
    Wait For Line On Uart    SWM boot    testerId=${gagan_uart}    timeout=10

Stage 2 — Sensor HAL Initialised
    Wait For Line On Uart    SWM sensor init    testerId=${gagan_uart}    timeout=5

Stage 3 — Power HAL Initialised
    Wait For Line On Uart    SWM power init    testerId=${gagan_uart}    timeout=5

Stage 4 — Comms HAL Initialised
    Wait For Line On Uart    SWM comms init    testerId=${gagan_uart}    timeout=10

Stage 5 — Hardware Ready
    [Documentation]    Last POST line — all HALs constructed, kernel built,
    ...                ready to enter the pairing handshake.
    Wait For Line On Uart    SWM hw ready    testerId=${gagan_uart}    timeout=10

Stage 6 — Pair Request Emitted
    [Documentation]    Confirms the FSM left Boot and reached the pairing
    ...                handshake (this is the on-target proxy for "actor
    ...                reached Idle" before any sensor cycle).
    Wait For Line On Uart    SWM PAIR request    testerId=${gagan_uart}    timeout=10

Stage 7 — System Did Not Abort During POST
    [Documentation]    No `_default_abort` symbol was hit — the firmware
    ...                did not panic or hit a backtrace at any point above.
    System Should Not Have Aborted

Stage 8 — QS Frames Were Emitted
    [Documentation]    EspQsSink::emit_frame fired at least once during the
    ...                POST sequence — the QS dictionary at startup plus any
    ...                actor transition guarantees this when QS is alive.
    QS Frames Should Have Been Seen
    [Teardown]    Reset Emulation
