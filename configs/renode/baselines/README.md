# Regression baselines (STS §10.1)

This directory holds the reference artifacts the `configs/renode/tests/regression/baselines.robot`
suite asserts against. They are the "expected behaviour at last good commit"
checkpoint — when firmware drift changes the captured output, the suite fails
and the operator decides whether to update the baseline (intentional change) or
fix the regression (unintentional).

Phase 5 of `docs/05-verification/TestPlan_SWM.md` introduced the mechanism.
The baselines are intentionally small — they only encode the **stable
prefixes** of the expected output, not the whole stream. This minimises
churn from incidental log-format changes while still catching:

- A boot-line dropping (UART regression);
- The wireless medium going silent (PCAP frame-count regression);
- A snapshot save/reload no longer continuing the run (snapshot
  regression — usually a peripheral-stub serialization bug).

## Files

| Baseline | Format | Asserted by |
|----------|--------|-------------|
| `txrx_uart.uart-expect.txt` | One expected line per row (prefix match, in order) | `regression/baselines.robot` → "UART Boot Sequence Matches Baseline" |
| `lora_traffic.frame_count` | Single integer — the minimum frame count tshark must parse out of `logs/baseline_lora.pcap` | `regression/baselines.robot` → "PCAP Frame Count Meets Baseline" |

Snapshot save/reload is purely runtime — the snapshot itself is written to
`target/test-results/regression/baselines/snapshot.dat` during the run and is
not checked in. The baseline is the *behaviour* (telemetry continues after
reload), not a byte-exact snapshot blob.

## Updating a baseline

1. Run `make renode-baselines` and confirm the suite fails for the expected
   reason (intentional firmware change).
2. Capture the new output (UART transcript or tshark frame count) and update
   the corresponding file in this directory.
3. Commit with a `test(baselines): …` message that explains *why* the
   baseline moved — future readers should be able to grep the log and see
   what changed.
4. CI should be green on the next run.

## Why these targets

- **UART**: the multinode `swm_multinode_lr1121.resc` platform is built without the
  `qs` feature (firmware feature set `lr1121,renode` — same as
  `test-renode-txrx` and `renode-actor-cycle`). QS HDLC binary frames over
  the USB Serial/JTAG would interleave 0x7E delimiter bytes into the
  `esp_println` stream and corrupt line-based UART parsing. The baseline
  therefore intentionally matches the txrx build, not the battery-fault
  qs build.
- **PCAP**: the multinode platform exposes the IEEE 802.15.4 medium as
  `wireless`; the suite enables `LogIEEE802_15_4Traffic` after the resc
  loads, so no firmware build flag drives PCAP. The floor is conservative
  (≥ 1 frame proves the medium is live); raise it deliberately once a
  stable run produces a higher count and the suite begins to mask
  regressions in frame production rate.
- **Snapshot**: same multinode platform; the OHT machine is saved, reset,
  reloaded, and the suite asserts a *new* telemetry frame appears after
  reload — proves the wireless stack and active-object state survive
  the round trip.
