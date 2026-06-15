*** Settings ***
Documentation     Shared keywords for the on-target actor/FSM suite (STS §6).
...
...               Provides the Phase-3 "productionised" form of the symbol-hook
...               decode pattern first prototyped in
...               integration/battery/fault.robot: scratchpad helpers, hook
...               installers, and the per-symbol counter helpers used by the
...               §6.1 POST, §6.9 watchdog, and §6.2/6.10 sensor-cycle suites.
...
...               The decode runs entirely from inside Renode hooks (no
...               monitor.Execute round-trip) so it is safe during
...               `emulation RunFor` — the same constraint the original
...               fault.robot suite documents.
Library           Process
Library           String
Resource          ${RENODEKEYWORDS}

*** Variables ***
${ACTOR_PROJECT_ROOT}      ${CURDIR}${/}..${/}..${/}..${/}..
${ACTOR_LR1121_RESC}       ${ACTOR_PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc
${ACTOR_MULTINODE_RESC}    ${ACTOR_PROJECT_ROOT}${/}configs${/}renode${/}swm${/}swm_multinode_lr1121.resc
${ACTOR_GAGAN_FW}          ${ACTOR_PROJECT_ROOT}${/}target${/}riscv32imac-unknown-none-elf${/}debug${/}swm-gagan-esp32c6

# Scratchpad slots inside spi_flash_stub (0x60002000, 0x1000 bytes). Each suite
# owns its own slot — the layout below is the union of slots used by §6 suites.
${ACTOR_SCRATCH_ABORT}        0x60002FF4
${ACTOR_SCRATCH_FRAMES}       0x60002FF0
${ACTOR_SCRATCH_WATCHDOG}     0x60002FE8

*** Keywords ***

# ── Setup ─────────────────────────────────────────────────────────────────────

Setup Actor Platform Single Node
    [Documentation]    Single-node OHT bring-up for §6.1-style assertions
    ...                that observe boot/POST and depend on `EspQsSink::
    ...                emit_frame` being present in the ELF (firmware must be
    ...                built with the `qs` feature). Mirrors the security-
    ...                suite pattern: load the LR1121 platform, silence
    ...                verbose logging, expose `${gagan_uart}`, install the
    ...                abort + emit_frame scratchpad hooks, then start one
    ...                continuously-running emulation.
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${ACTOR_PROJECT_ROOT}')"
    Execute Command    include @${ACTOR_LR1121_RESC}
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    logLevel 0
    ${gagan_uart}=     Create Terminal Tester    sysbus.usb_serial_jtag    machine=ESP32-C6-DevKit-LR1121
    Set Suite Variable    ${gagan_uart}
    Install Abort Hook    ${ACTOR_SCRATCH_ABORT}
    Install QS Frame Hook    ${ACTOR_SCRATCH_FRAMES}
    Start Emulation

Setup Actor Platform Multi Node
    [Documentation]    Two-node OHT+MC bring-up for §6.2 / §6.9 / §6.10
    ...                sensor-cycle assertions that need pairing to complete
    ...                before the telemetry loop starts. Does NOT enable
    ...                `qs` — `EspQsSink` writes HDLC binary frames to the
    ...                same USB Serial/JTAG that `esp_println` uses, which
    ...                corrupts the line-based UART tester (interleaved
    ...                0x7E bytes split text lines). Builds for this setup
    ...                use `lr1121,renode` only; the abort hook is still
    ...                installed (always-present `_default_abort` symbol),
    ...                the QS frame hook is not.
    Execute Command    python "import System.IO; System.IO.Directory.SetCurrentDirectory('${ACTOR_PROJECT_ROOT}')"
    Execute Command    include @${ACTOR_MULTINODE_RESC}
    Execute Command    mach set "SWM-Gagan-OHT"
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    Execute Command    mach set "SWM-Pramukh-MC"
    Execute Command    sysbus LogAllPeripheralsAccess false
    Execute Command    cpu0 LogFunctionNames false
    ${gagan_uart}=     Create Terminal Tester    sysbus.usb_serial_jtag    machine=SWM-Gagan-OHT
    ${pramukh_uart}=   Create Terminal Tester    sysbus.usb_serial_jtag    machine=SWM-Pramukh-MC
    Set Suite Variable    ${gagan_uart}
    Set Suite Variable    ${pramukh_uart}
    Execute Command    mach set "SWM-Gagan-OHT"
    Install Abort Hook    ${ACTOR_SCRATCH_ABORT}
    Start Emulation

# ── Symbol discovery ──────────────────────────────────────────────────────────

Get Actor Symbol Address
    [Documentation]    Resolve a symbol matching ${grep_pattern} from the
    ...                Gagan ELF via `nm`. Works for both global (T) symbols
    ...                via AddSymbolHook and local (t) symbols (which are
    ...                invisible to AddSymbolHook) when paired with AddHook.
    [Arguments]        ${grep_pattern}
    ${result}=         Run Process    sh    -c    nm ${ACTOR_GAGAN_FW} | grep "${grep_pattern}"
    ...                stdout=PIPE    stderr=PIPE
    Should Be Equal As Integers    ${result.rc}    0
    ...    msg=Symbol matching '${grep_pattern}' not found in ${ACTOR_GAGAN_FW}
    ${first_line}=     Get Line    ${result.stdout}    0
    ${parts}=          Split String    ${first_line.strip()}
    RETURN             0x${parts}[0]

# ── Hook installers ───────────────────────────────────────────────────────────

Install Abort Hook
    [Documentation]    Wire `_default_abort` (global symbol — AddSymbolHook
    ...                works directly) to set the scratch flag when the
    ...                firmware panics. Hooks write to memory only — no
    ...                monitor.Execute call — so they are safe during RunFor.
    [Arguments]        ${scratch_addr}
    Execute Command    sysbus WriteDoubleWord ${scratch_addr} 0
    Execute Command    cpu0 AddSymbolHook "_default_abort" "machine.SystemBus.WriteDoubleWord(${scratch_addr}, 1)"

Install QS Frame Hook
    [Documentation]    Wire `EspQsSink::emit_frame` (local `t` symbol — must
    ...                use AddHook by address) to increment the scratch
    ...                "frames seen" counter on every QS record emission.
    [Arguments]        ${scratch_addr}
    Execute Command    sysbus WriteDoubleWord ${scratch_addr} 0
    ${addr}=           Get Actor Symbol Address    EspQsSink.*emit_frame
    Execute Command    cpu0 AddHook ${addr} "machine.SystemBus.WriteDoubleWord(${scratch_addr}, machine.SystemBus.ReadDoubleWord(${scratch_addr}) + 1)"

Install Counter Hook For Symbol
    [Documentation]    Install an AddHook on the first symbol matching
    ...                ${grep_pattern} that increments the scratch counter at
    ...                ${scratch_addr}. Used for any §6 symbol whose call
    ...                count is the assertion (e.g. EspPower::kick_watchdog).
    [Arguments]        ${grep_pattern}    ${scratch_addr}
    Execute Command    sysbus WriteDoubleWord ${scratch_addr} 0
    ${addr}=           Get Actor Symbol Address    ${grep_pattern}
    Execute Command    cpu0 AddHook ${addr} "machine.SystemBus.WriteDoubleWord(${scratch_addr}, machine.SystemBus.ReadDoubleWord(${scratch_addr}) + 1)"

# ── Timing ────────────────────────────────────────────────────────────────────

Run Virtual Ms
    [Arguments]        ${ms}
    Execute Command    emulation RunFor "00:00:00.${ms}"

Run Virtual Seconds
    [Arguments]        ${seconds}
    Execute Command    emulation RunFor "${seconds}"

# ── Scratch I/O ───────────────────────────────────────────────────────────────

Read Scratch Counter
    [Arguments]        ${scratch_addr}
    ${raw}=            Execute Command    sysbus ReadDoubleWord ${scratch_addr}
    ${v}=              Convert To Integer    ${raw.strip()}
    RETURN             ${v}

Reset Scratch Counter
    [Arguments]        ${scratch_addr}
    Execute Command    sysbus WriteDoubleWord ${scratch_addr} 0

# ── Assertions ────────────────────────────────────────────────────────────────

System Should Not Have Aborted
    ${v}=              Read Scratch Counter    ${ACTOR_SCRATCH_ABORT}
    Should Be Equal As Integers    ${v}    0
    ...    msg=_default_abort fired — firmware panicked or hit a backtrace.

QS Frames Should Have Been Seen
    [Documentation]    At least one QS actor/session record was emitted —
    ...                proves the FSM is making transitions, not silently
    ...                wedged. The hook counts every emit_frame call so any
    ...                non-zero value satisfies the assertion.
    ${v}=              Read Scratch Counter    ${ACTOR_SCRATCH_FRAMES}
    Should Be True    ${v} > 0
    ...    msg=No QS frames emitted — emit_frame was never called.

Counter Should Be At Least
    [Arguments]        ${scratch_addr}    ${expected}    ${label}
    ${v}=              Read Scratch Counter    ${scratch_addr}
    Should Be True    ${v} >= ${expected}
    ...    msg=${label} expected at least ${expected}, got ${v}

Counter Should Be Between
    [Arguments]        ${scratch_addr}    ${lo}    ${hi}    ${label}
    ${v}=              Read Scratch Counter    ${scratch_addr}
    Should Be True    ${v} >= ${lo}
    ...    msg=${label} below floor ${lo}: got ${v}
    Should Be True    ${v} <= ${hi}
    ...    msg=${label} above ceiling ${hi}: got ${v}
