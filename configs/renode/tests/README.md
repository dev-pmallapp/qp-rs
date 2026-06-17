# Renode simulation tests

Robot Framework suites for SWM firmware, organized into a layered hierarchy that
mirrors the platform model (`isa/ → soc/ → platform/`). See
[`docs/05-verification/TestingTopics.md`](../../../docs/05-verification/TestingTopics.md) for
the design rationale and the migration history.

## Directory layout

```
configs/renode/tests/
├─ peripherals/   register-level model tests (no firmware)
│   ├─ lora/      lr1121_protocol.robot  (+ test_lora_swm_platform.resc)
│   ├─ battery/   battery_adc.robot
│   └─ timer/     systimer.robot
├─ soc/           SoC bring-up / memory-map smoke tests
│   └─ esp32c6/   runfor_hooks.robot
├─ platform/      platform-description / boot smoke tests
│   ├─ descriptions.robot
│   └─ performance.robot
├─ smoke/         harness mechanics (connection / load / teardown)
│   ├─ connection.robot   platform_load.robot   teardown.robot
│   └─ debug_*.robot      interactive diagnostic aids (excluded from renode-test-all)
└─ integration/   multi-node firmware flows
    ├─ fota/      fota.robot      (Demo 2 — pairing + version-gated FOTA, STAGED)
    ├─ txrx/      txrx.robot      (Demo 1 — OHT→MC telemetry, STAGED)
    ├─ lora/      multinode.robot
    └─ battery/   fault.robot  (+ fault.resc)
```

Shared keyword resources live in `configs/renode/shared/robot-keywords/`
(`common_keywords.robot`, `fota_keywords.robot`, `txrx_keywords.robot`) — **not**
under `tests/`, which holds only runnable suites.

## Running

All `make` targets route output to `target/test-results/<suite>/` (never the repo
root). Run from the repo root.

```sh
make renode-test-all        # walk the hierarchy; run every suite, continue on failure
make renode-test            # the primary battery-fault suite (alias)

# Individual suites
make renode-battery-fault   # integration/battery/fault.robot
make renode-battery-adc     # peripherals/battery/battery_adc.robot
make renode-lora-swm        # peripherals/lora/lr1121_protocol.robot
make renode-lora-multinode  # integration/lora/multinode.robot
make renode-platform-desc   # platform/descriptions.robot
make renode-performance     # platform/performance.robot
make renode-systimer        # peripherals/timer/systimer.robot
make renode-step1 .. step4  # smoke/ + soc/ harness checks

# Staged integration flows (rebuild firmware first — see below)
make test-renode-txrx       # integration/txrx/txrx.robot  (Demo 1)
make test-renode-fota       # integration/fota/fota.robot  (Demo 2)
```

`renode-test-all` discovers suites by walking `peripherals/ soc/ platform/
smoke/ integration/`. It deliberately skips the staged flows (`integration/txrx`,
`integration/fota`) and the `debug_*` aids — see the staging note below.

Direct invocation also works (you must supply `RENODEKEYWORDS` if you bypass
`renode-test`):

```sh
renode-test \
  --outputdir target/test-results/integration/battery/fault \
  configs/renode/tests/integration/battery/fault.robot
```

## The staged-flow convention (integration/txrx, integration/fota)

Long multi-node flows are decomposed into **ordered stages** so each milestone
gets its own pass/fail/timing line in the report — a failing stage pinpoints
exactly where the flow breaks.

Renode normally chains stages with `Provides`/`Requires` snapshots, but the SWM
wireless stack (`LR1121Radio` + the IEEE 802.15.4 medium) **does not serialize**,
so snapshots are unusable here. Instead each flow is a **single `.robot` file**
whose stages are ordered test cases sharing **one live emulation**:

- `Test Setup` / `Test Teardown` are `No Operation`, so the emulation is never
  reset between cases.
- Stage 1 runs `Setup <Flow> Platform` (load the `.resc`, silence logging, create
  global UART testers, `Start Emulation`) and asserts the boot milestone.
- Each later stage asserts the next milestone in the firmware's Active-Object
  progression. **Cases must not be reordered.**
- The final stage's `[Teardown]` runs the single `Reset Emulation` for a clean exit.

Both flows need firmware built with flow-specific cargo features, which is why
they have dedicated `make` targets rather than being part of `renode-test-all`:
`test-renode-txrx` builds with `--features lr1121,renode`; `test-renode-fota`
adds `fota-demo` (lowers the OHT's reported `fw_ver` below the MC target so the
version gate opens). Milestone keywords live in
`shared/robot-keywords/<flow>_keywords.robot`.

To stage a new flow: split its asserts into milestone keywords in a new
`shared/robot-keywords/<flow>_keywords.robot`, add `Setup <Flow> Platform`
(with `Start Emulation`), write `integration/<flow>/<flow>.robot` with the
`No Operation` setup/teardown and one ordered test case per milestone, and add a
`test-renode-<flow>` target that builds firmware then runs the suite.

## Battery fault suite (`integration/battery/fault.robot`)

A standalone manual smoke run, watching QS frames in qspy:

```sh
qspy -t -p 7777                                          # terminal 1
renode configs/renode/tests/integration/battery/fault.resc   # terminal 2
```

### What it checks

| Test | Fault path | Assertion |
|------|-----------|-----------|
| Normal Boot Emits QS Frames | none | ≥1 QS frame within 3 virtual seconds |
| Battery Hard Fault … | 1000 mV (below the 2400 mV floor) | more QS frames after injection; no abort |
| Battery Low SoC … | 2500 mV (low % path) | more QS frames; no abort |
| Battery Restored … | fault then restore | QS frames resume after restore |

### How it works

1. `emulation RunFor` advances virtual time deterministically — the suite
   finishes in real seconds, not minutes.
2. A symbol hook on `EspQsSink::emit_frame` (resolved from the ELF via `nm`)
   counts QS frames into a Renode variable.
3. A hook on `_default_abort` flags any panic so the test fails immediately
   instead of timing out.
4. Fault injection writes to `adc_stub` magic register `0x6000EFF0`
   (see `battery_adc_stub.py` for the register map).

To assert on specific state transitions rather than just counting frames, add a
symbol hook on `EspQsSink::actor_transition` (find the mangled name with `nm`
over the ELF) and decode the arguments via the RISC-V `a0`–`a4` registers or the
QS HDLC stream in qspy.
