*** Settings ***
Documentation     STS §9 security catalog, asserted on-target. The Gagan firmware
...               built with the sec-demo feature runs the swm-protocol::security
...               replay/auth/expiry vectors on the real RISC-V core at boot and
...               prints one SWM SEC line per §9 row; each stage below asserts the
...               next line, so a failing stage pinpoints which check regressed.
...
...               This is the L-S (simulation) twin of the swm-comms host #[test]s
...               in crates/swm-comms/src/security.rs — the same logic verified at
...               the cheapest level (host) is re-verified on-target here.
...
...               Build prerequisite (from repo root):
...                 make renode-security
...               which builds the ESP32-C6 ELF with the `lr1121,renode,sec-demo`
...               cargo features and runs this suite.
...
...               The self-test prints all lines during one boot, so the stages
...               share ONE live emulation: Test Setup/Teardown are No Operation so
...               the emulation is not reset between cases, and Robot runs cases in
...               file order. The stages below are ordered and must not be reordered.
Test Setup        No Operation
Test Teardown     No Operation
Resource          ${RENODEKEYWORDS}
Resource          ${CURDIR}/../../../shared/robot-keywords/security_keywords.robot
Test Timeout      90 seconds

*** Test Cases ***

Stage 1 — Node Boots And Runs Self-Test
    [Documentation]    Bring up the platform, start the shared emulation, and reach
    ...                the boot banner; the security self-test runs immediately after.
    Setup Security Platform
    Wait For Boot

Stage 2 — Authentic Command Accepted
    [Documentation]    Baseline: a fresh, correctly-keyed command verifies and is
    ...                accepted.
    Wait For Command Accepted

Stage 3 — Replay Attack Rejected (9.1)
    [Documentation]    A frame whose counter does not advance is rejected by the
    ...                frame-counter regression check.
    Wait For Replay Rejected

Stage 4 — Forged Source Rejected (9.2)
    [Documentation]    A command signed with a key the sender should not hold fails
    ...                authentication at the MAC layer.
    Wait For Forged Source Rejected

Stage 5 — Invalid Join Credentials Rejected (9.3)
    [Documentation]    A pairing HMAC computed under the wrong key does not verify.
    Wait For Invalid Join Rejected

Stage 6 — Unauthorized Command Rejected (9.4)
    [Documentation]    A tampered command body invalidates the authentication tag.
    Wait For Unauthorized Command Rejected

Stage 7 — Expired Command Rejected (9.5 / VVT-004)
    [Documentation]    A command whose expiry_unix_s has passed is rejected.
    Wait For Expired Command Rejected
    Wait For Selftest Complete
    [Teardown]    Reset Emulation
