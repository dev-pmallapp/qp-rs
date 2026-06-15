*** Settings ***
Documentation     STS §6.2 (sensor cycle — mid level), §6.3 (low level),
...               §6.4 (full level), §6.6 (telemetry failure → comm fault),
...               §6.8 (solar charging detect), §6.9 (watchdog kick per
...               cycle), §6.10 (wake scheduling accuracy), and §6.11
...               (brownout flag surfaces on power status) on-target.
...
...               After pairing completes, the OHT enters the sample/process/
...               transmit loop and prints one `SWM TX telemetry level=..%
...               dist=..mm seq=N` line per cycle. This suite is intentionally
...               cheap — it mirrors `integration/txrx/txrx.robot` and adds
...               extra assertions on the same continuously-running emulation
...               (a second cycle inside the bounded wake window, two
...               distance-injection cycles for §6.3/§6.4, and the LR1121-
...               model TxCount as the §6.9 vehicle) so it does not pay
...               multiple multi-node boot/pair budgets.
...
...               §6.2  — the line appears within the post-pairing window
...                       (proves Sampling → Processing → Transmitting runs
...                       on-target).
...               §6.10 — a second telemetry line appears within the bounded
...                       wake interval (`bounded_bring_up_delay_ms` in
...                       ports/esp32c6/src/bin/oht.rs clamps the wake
...                       delay to 1 s ≤ d ≤ 5 s). Two cycles within 30 s
...                       of wall-clock is the on-target proxy for the host
...                       wake-scheduler ±1-tick assertion; the host-level
...                       tests are exhaustive on the cap and floor logic,
...                       so this is the smoke-level on-target reassurance.
...               §6.3  — Stage 3 writes the Phase 3.5.3 distance-injection
...                       static (`RENODE_INJECTED_DISTANCE_MM` in
...                       `ports/esp32c6/src/board/sensor.rs`) to a near-
...                       empty value (1900 mm with the default calibration
...                       empty=2000/full=200) via the ELF symbol address
...                       (resolved through `Get Actor Symbol Address`),
...                       then waits for a telemetry line whose `level=N%`
...                       falls inside the LevelState::Low band (≤ 20 %,
...                       OHT default low_threshold_percent). Proves the
...                       Sensor → Processor → Telemetry path classifies low
...                       on-target — the wake-band cap itself is exhausted
...                       by host tests, since `bounded_bring_up_delay_ms`
...                       compresses the actual on-target cadence into the
...                       1–5 s smoke window regardless of band.
...               §6.4  — Stage 4 reuses the same injection static to pin
...                       distance into the clamped-full region (100 mm <
...                       full_distance_mm = 200, so the processor clamps
...                       to 200 and computes 100 % filled), then waits for
...                       `level=100%` — on-target proof that the Full
...                       classification path runs end-to-end.
...               §6.9  — the watchdog kick path
...                       (`PowerManager::status → power.kick_watchdog`)
...                       runs once per cycle, on the same code path as the
...                       LoRa SetTx that produces each `SWM TX telemetry`
...                       line. Phase 3.5.1 extended the LR1121 Renode
...                       model with a virtual TxCount register at SPI base
...                       + 0xFF0 (no CPU hook, no per-fire pause); Stage 5
...                       reads it after four telemetry lines (Stage 2 ×2 +
...                       Stage 3 + Stage 4) and asserts >= 2. Equivalent-
...                       strength on-target proof that the shared cycle
...                       code path executed without the simulation slowdown
...                       an AddHook on `kick_watchdog` would incur.
...               §6.11 — Stage 6 writes the Phase 3.5.4 brownout-injection
...                       static (`RENODE_BROWNOUT_SEEN` in
...                       `ports/esp32c6/src/board/power.rs`) via its ELF
...                       symbol address, then waits for the next
...                       telemetry cycle to land with `flags=0x09` (bit 0
...                       ext_pwr | bit 3 brownout). The
...                       `power_flags=...` byte is decoded straight out
...                       of payload[4] in `EspLoraComms::send`, so the
...                       UART substring is the on-target proof that
...                       `EspPower::read_status` carried the brownout
...                       bit through `PowerStatus.brownout_seen` end-to-
...                       end.
...               §6.8  — Stage 7 writes the Phase 3.5.5 solar-injection
...                       static (`RENODE_SOLAR_PRESENT` in the same file
...                       as `RENODE_BROWNOUT_SEEN`), then waits for the
...                       next telemetry cycle to land with `flags=0x0B`
...                       (bit 0 ext_pwr | bit 1 solar | bit 3 brownout,
...                       since Stage 6 still has brownout set). Same
...                       end-to-end path as §6.11 — proves bit 1 of the
...                       power_flags byte plumbs through.
...               §6.6  — Stage 8 writes the Phase 3.5.6 LR1121-model
...                       force-tx-fail count register at SPI base + 0xFF4
...                       on the OHT (`configs/renode/shared/peripherals/
...                       lora/lr1121_radio.cs`); the model drops the
...                       next SetTx's IRQ_TXDONE and FrameSent so the
...                       firmware's GetIrqStatus poll times out and
...                       `EspLoraComms::send` returns `Err(IoError)`.
...                       The send path prints `SWM TX failed seq=N` on
...                       the error branch, which is the on-target proof
...                       that the telemetry-failure path is reachable
...                       from a deterministic stimulus.
...
...               Build prerequisite (from repo root):
...                   make renode-actor-cycle
Test Setup        No Operation
Test Teardown     No Operation
Resource          ${RENODEKEYWORDS}
Resource          ${CURDIR}/../../../shared/robot-keywords/actor_keywords.robot
Test Timeout      90 seconds

*** Variables ***
# LR1121 model virtual TxCount register on the OHT (SWM-Gagan-OHT).
# SPI2 base is 0x60081000 (see configs/renode/swm/swm_multinode_lr1121.resc);
# offset 0xFF0 is the Phase 3.5.1 model-only register — see
# configs/renode/shared/peripherals/lora/lr1121_radio.cs.
${OHT_LR1121_TXCOUNT}    0x60081FF0

# `nm` grep pattern that resolves the Phase 3.5.3 distance-injection static
# in the OHT ELF (Rust-mangled symbol, single hit). Defined in
# `ports/esp32c6/src/board/sensor.rs` under `cfg(feature = "renode")`.
${OHT_HCSR04_INJECT_SYMBOL}    RENODE_INJECTED_DISTANCE_MM

# Distances chosen for §6.3 / §6.4 against the OHT default calibration
# (empty_distance_mm=2000, full_distance_mm=200, low_threshold_percent=20):
#   1900 mm → filled=100 mm → 5 % → LevelState::Low  (≤ 20 % threshold)
#    100 mm → clamped to 200 → filled=1800 mm → 100 % → LevelState::Full
${HCSR04_DISTANCE_LOW_MM}     1900
${HCSR04_DISTANCE_FULL_MM}    100

# `nm` grep pattern that resolves the Phase 3.5.4 brownout-injection
# static in the OHT ELF (Rust-mangled symbol, single hit). Defined in
# `ports/esp32c6/src/board/power.rs` under `cfg(feature = "renode")`.
${OHT_BROWNOUT_SYMBOL}    RENODE_BROWNOUT_SEEN

# `nm` grep pattern that resolves the Phase 3.5.5 solar-injection static
# in the OHT ELF (Rust-mangled symbol, single hit). Defined alongside
# `RENODE_BROWNOUT_SEEN` in `ports/esp32c6/src/board/power.rs`.
${OHT_SOLAR_SYMBOL}    RENODE_SOLAR_PRESENT

# LR1121 model force-tx-fail count virtual register on the OHT
# (SWM-Gagan-OHT). Model-only, outside the real GPSPI2 layout — see
# Phase 3.5.6 in configs/renode/shared/peripherals/lora/lr1121_radio.cs.
# Writing N drops the next N SetTx operations (no IRQ_TXDONE, no
# FrameSent), forcing the firmware's transmit poll loop to time out.
${OHT_LR1121_FORCE_FAIL}    0x60081FF4

*** Test Cases ***

Stage 1 — Both Nodes Boot And Pair
    [Documentation]    Bring up the multi-node emulation, wait for both
    ...                boot banners, and wait for pairing to complete —
    ...                pairing is a prerequisite for the OHT sample loop.
    Setup Actor Platform Multi Node
    Wait For Line On Uart    SWM boot       testerId=${gagan_uart}      timeout=10
    Wait For Line On Uart    SWM boot       testerId=${pramukh_uart}    timeout=10
    Wait For Line On Uart    SWM PAIR ok    testerId=${gagan_uart}      timeout=30

Stage 2 — Sensor Cycle Telemetry Recurs Within Bounded Wake Interval
    [Documentation]    Two consecutive `SWM TX telemetry` lines must land
    ...                — the first proves the §6.2 Sampling → Processing →
    ...                Transmitting path runs on-target, the second proves
    ...                §6.10 the wake-scheduler advanced into the next
    ...                cycle inside the bounded interval. MC RX is the
    ...                round-trip sanity guard; the abort flag (installed
    ...                by Setup Actor Platform Multi Node) catches a panic
    ...                anywhere along the way.
    Wait For Line On Uart    SWM TX telemetry    testerId=${gagan_uart}    timeout=20
    Wait For Line On Uart    SWM TX telemetry    testerId=${gagan_uart}    timeout=30
    Wait For Line On Uart    SWM RX telemetry    testerId=${pramukh_uart}    timeout=30
    System Should Not Have Aborted

Stage 3 — Low Level Echo Triggers Low Classification (§6.3)
    [Documentation]    Inject a near-empty distance into the Phase 3.5.3
    ...                static `RENODE_INJECTED_DISTANCE_MM` (resolved via
    ...                `nm`, since the Rust symbol is mangled), then wait
    ...                for the next telemetry cycle to carry the low-band
    ...                reading. The level percent printed by
    ...                `EspLoraComms::send` comes from the same
    ...                `LevelReading` the processor produced, so observing
    ...                `level=5%` on UART proves the Sensor → Processor →
    ...                Telemetry pipeline classified the injected sample as
    ...                Low end-to-end.
    Execute Command    mach set "SWM-Gagan-OHT"
    ${inject_addr}=    Get Actor Symbol Address    ${OHT_HCSR04_INJECT_SYMBOL}
    Execute Command    sysbus WriteDoubleWord ${inject_addr} ${HCSR04_DISTANCE_LOW_MM}
    Wait For Line On Uart    SWM TX telemetry level=5%    testerId=${gagan_uart}    timeout=20
    System Should Not Have Aborted

Stage 4 — Full Level Echo Triggers Full Classification (§6.4)
    [Documentation]    Reuse the injection static to pin distance into the
    ...                clamped-full region (100 mm < `full_distance_mm`,
    ...                so the processor clamps to 200 mm and reports
    ...                100 %). Waiting for `level=100%` on UART proves the
    ...                same pipeline classifies the injected sample as Full
    ...                on-target. Sequential to Stage 3 so the suite pays
    ...                one platform-bring-up cost for both.
    Execute Command    mach set "SWM-Gagan-OHT"
    ${inject_addr}=    Get Actor Symbol Address    ${OHT_HCSR04_INJECT_SYMBOL}
    Execute Command    sysbus WriteDoubleWord ${inject_addr} ${HCSR04_DISTANCE_FULL_MM}
    Wait For Line On Uart    SWM TX telemetry level=100%    testerId=${gagan_uart}    timeout=20
    System Should Not Have Aborted

Stage 5 — Watchdog Path Executed (§6.9)
    [Documentation]    Read the LR1121-model TxCount virtual register on
    ...                the OHT and assert it observed at least the two
    ...                TXs that Stage 2 already proved arrived at the MC
    ...                (the actual count is higher because Stages 3 and 4
    ...                drove additional cycles, but >= 2 is the §6.9
    ...                load-bearing assertion). `PowerManager::status →
    ...                power.kick_watchdog` shares the cycle code path
    ...                with the LoRa SetTx call site, so TxCount >= 2 is
    ...                the on-target proof that the watchdog kick path
    ...                executed each cycle — without paying the per-fire
    ...                CPU pause that an AddHook on `kick_watchdog`
    ...                itself would incur. See Phase 3.5.1 in
    ...                docs/05-verification/TestPlan_SWM.md for the
    ...                model-side register layout (offset 0xFF0).
    Execute Command    mach set "SWM-Gagan-OHT"
    Counter Should Be At Least    ${OHT_LR1121_TXCOUNT}    2    OHT LR1121 TxCount

Stage 6 — Brownout Flag Surfaces On Power Status (§6.11)
    [Documentation]    Inject brownout-seen=true via the Phase 3.5.4
    ...                static `RENODE_BROWNOUT_SEEN` (resolved via `nm`,
    ...                since the Rust symbol is mangled). The next
    ...                `EspPower::read_status` carries the bit through
    ...                `PowerStatus.brownout_seen` → `power_flags` byte
    ...                3 → the `flags=0x09` substring on the SWM TX
    ...                telemetry anchor (bit 0 ext_pwr | bit 3 brownout).
    ...                Sequential to Stage 4, so the injection static
    ...                still holds 100 mm — the level processor clamps
    ...                that into `percent_full=100`, but `raw_distance_mm`
    ...                on the wire is the unmodified sample (100 mm), so
    ...                the expected anchor is `level=100% dist=100mm
    ...                flags=0x09`. Only the flags byte changes vs. the
    ...                Stage 4 cycle.
    Execute Command    mach set "SWM-Gagan-OHT"
    ${inject_addr}=    Get Actor Symbol Address    ${OHT_BROWNOUT_SYMBOL}
    Execute Command    sysbus WriteByte ${inject_addr} 1
    Wait For Line On Uart    SWM TX telemetry level=100% dist=100mm flags=0x09    testerId=${gagan_uart}    timeout=20
    System Should Not Have Aborted

Stage 7 — Solar Present Flag Surfaces On Power Status (§6.8)
    [Documentation]    Inject solar_present=true via the Phase 3.5.5
    ...                static `RENODE_SOLAR_PRESENT` (resolved via `nm`,
    ...                same pattern as `RENODE_BROWNOUT_SEEN`). The next
    ...                `EspPower::read_status` carries the bit through
    ...                `PowerStatus.solar_present` → `power_flags` byte
    ...                bit 1 → the `flags=0x0B` substring on the SWM TX
    ...                telemetry anchor (bit 0 ext_pwr | bit 1 solar |
    ...                bit 3 brownout, since Stage 6 still has brownout
    ...                set). Sequential to Stage 6, so the distance and
    ...                level percent are unchanged (100 mm, 100 %); only
    ...                bit 1 of the flags byte changes.
    Execute Command    mach set "SWM-Gagan-OHT"
    ${inject_addr}=    Get Actor Symbol Address    ${OHT_SOLAR_SYMBOL}
    Execute Command    sysbus WriteByte ${inject_addr} 1
    Wait For Line On Uart    SWM TX telemetry level=100% dist=100mm flags=0x0B    testerId=${gagan_uart}    timeout=20
    System Should Not Have Aborted

Stage 8 — Telemetry Failure Path Reaches Comm Fault (§6.6)
    [Documentation]    Write the Phase 3.5.6 LR1121-model force-tx-fail
    ...                count register at SPI base + 0xFF4 with value 1.
    ...                The model drops the next SetTx's IRQ_TXDONE and
    ...                FrameSent, so the firmware's transmit poll loop
    ...                in crates/swm-drivers/src/lora.rs runs out of
    ...                retries (~5000 × ~56 µs ≈ 280 ms) and returns
    ...                Status::Timeout. `EspLoraComms::send` then prints
    ...                the `SWM TX failed seq=N` anchor on the error
    ...                branch (only for telemetry MessageKind, so it
    ...                doesn't fire on transient pairing-handshake
    ...                misses). One observation is enough — recovery is
    ...                exhausted by host tests; the on-target assertion
    ...                is just that the failure path is reachable from a
    ...                deterministic stimulus.
    Execute Command    mach set "SWM-Gagan-OHT"
    Execute Command    sysbus WriteDoubleWord ${OHT_LR1121_FORCE_FAIL} 1
    Wait For Line On Uart    SWM TX failed    testerId=${gagan_uart}    timeout=30
    System Should Not Have Aborted
    [Teardown]    Reset Emulation
