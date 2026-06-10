//! QUTest integration tests for the DPP example.
//!
//! These tests verify the full QUTest infrastructure round-trip:
//!  1. The RX parser correctly decodes all QUTest commands.
//!  2. The probe registry stores and consumes probes correctly.
//!  3. The TEST_PROBE record payload format is compatible with QSpy.
//!
//! A real QUTest session would run through QSpy (the host tool), which sends
//! HDLC-framed RX commands over the command TCP channel.  These tests exercise
//! the same code paths without a network connection.

use qs::qutest::{clear_test_probes, make_probe_record, set_test_probe, take_test_probe};
use qs::rx::{cmd, RxCmd, RxParser};

// ── HDLC frame encoder (mirrors tools/qspy/src/commands.rs) ──────────────────

fn encode_frame(seq: u8, cmd_id: u8, payload: &[u8]) -> Vec<u8> {
    const FLAG: u8 = 0x7E;
    const ESC: u8 = 0x7D;
    const XOR: u8 = 0x20;
    let mut raw = vec![seq, cmd_id];
    raw.extend_from_slice(payload);
    let sum: u8 = raw.iter().fold(0, |a, &b| a.wrapping_add(b));
    raw.push(!sum);
    let mut frame = vec![FLAG];
    for byte in raw {
        if byte == FLAG || byte == ESC {
            frame.push(ESC);
            frame.push(byte ^ XOR);
        } else {
            frame.push(byte);
        }
    }
    frame.push(FLAG);
    frame
}

// ── Probe registry tests ──────────────────────────────────────────────────────

#[test]
fn test_probe_set_take_cycle() {
    clear_test_probes();
    set_test_probe(0x1000_0001, 42);
    assert_eq!(take_test_probe(0x1000_0001), Some(42), "first take should return data");
    assert_eq!(take_test_probe(0x1000_0001), None, "second take should be empty (consumed)");
    clear_test_probes();
}

#[test]
fn test_probe_overwrite_keeps_latest() {
    clear_test_probes();
    set_test_probe(0x1000_0002, 10);
    set_test_probe(0x1000_0002, 20);
    assert_eq!(take_test_probe(0x1000_0002), Some(20));
    clear_test_probes();
}

#[test]
fn test_setup_via_rx_clears_probes() {
    clear_test_probes();
    set_test_probe(0x1000_0003, 99);

    // Decode a TEST_SETUP command and simulate the target handler
    let frame = encode_frame(1, cmd::TEST_SETUP, &[]);
    let cmds = RxParser::new().push_slice(&frame);
    assert_eq!(cmds, vec![RxCmd::TestSetup]);

    // The DPP handler calls clear_test_probes() when it receives TestSetup
    clear_test_probes();

    assert_eq!(take_test_probe(0x1000_0003), None, "probe cleared by TestSetup");
}

#[test]
fn test_probe_via_rx_sets_registry() {
    clear_test_probes();

    let fn_handle: u64 = 0xCAFE_BABE_0000_0001;
    let probe_data: u32 = 0x1234_5678;

    // Encode a TEST_PROBE command (8-byte fn_ptr + 4-byte data)
    let mut payload = [0u8; 12];
    payload[0..8].copy_from_slice(&fn_handle.to_le_bytes());
    payload[8..12].copy_from_slice(&probe_data.to_le_bytes());

    let frame = encode_frame(2, cmd::TEST_PROBE, &payload);
    let cmds = RxParser::new().push_slice(&frame);

    match &cmds[..] {
        [RxCmd::TestProbe { fn_ptr, data }] => {
            // Simulate the DPP handler: register the probe
            set_test_probe(*fn_ptr, *data);
        }
        other => panic!("unexpected: {other:?}"),
    }

    assert_eq!(take_test_probe(fn_handle), Some(probe_data));

    clear_test_probes();
}

#[test]
fn test_teardown_via_rx_clears_probes() {
    clear_test_probes();
    set_test_probe(0x1000_0004, 7);

    let frame = encode_frame(3, cmd::TEST_TEARDOWN, &[]);
    let cmds = RxParser::new().push_slice(&frame);
    assert_eq!(cmds, vec![RxCmd::TestTeardown]);

    clear_test_probes();  // handler action

    assert_eq!(take_test_probe(0x1000_0004), None);
}

#[test]
fn test_continue_decodes_correctly() {
    let frame = encode_frame(1, cmd::TEST_CONTINUE, &[]);
    let cmds = RxParser::new().push_slice(&frame);
    assert_eq!(cmds, vec![RxCmd::TestContinue]);
}

// ── TEST_PROBE_GET record format test ─────────────────────────────────────────

#[test]
fn probe_record_payload_matches_qspy_expectation() {
    // QSpy interpreter expects: [fn_ptr: 8 LE bytes] [data: 4 LE bytes]
    let fn_ptr: u64 = 0x0102_0304_0506_0708;
    let data: u32   = 0x0A0B_0C0D;
    let rec = make_probe_record(fn_ptr, data);

    assert_eq!(rec.len(), 12);
    assert_eq!(u64::from_le_bytes(rec[0..8].try_into().unwrap()), fn_ptr);
    assert_eq!(u32::from_le_bytes(rec[8..12].try_into().unwrap()), data);
}

// ── Event injection decoding ──────────────────────────────────────────────────

#[test]
fn event_injection_command_decodes() {
    // prio=3, signal=5 (EAT_SIG in DPP), no payload
    let payload = [3u8, 5, 0];
    let frame = encode_frame(1, cmd::EVENT, &payload);
    let cmds = RxParser::new().push_slice(&frame);
    assert_eq!(cmds, vec![RxCmd::Event { prio: 3, signal: 5, payload: vec![] }]);
}
