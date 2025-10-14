//! Memory management tests for qp-mem

use qp_mem::{QPoolStats, define_event};
use qp_core::{QEvent, QSignal};

#[test]
fn test_pool_stats_creation() {
    let stats = QPoolStats::new(10);
    assert_eq!(stats.total_blocks, 10);
    assert_eq!(stats.free_blocks, 10);
    assert_eq!(stats.used_blocks, 0);
}

#[test]
fn test_pool_stats_alloc() {
    let mut stats = QPoolStats::new(10);
    stats.on_alloc();
    assert_eq!(stats.free_blocks, 9);
    assert_eq!(stats.used_blocks, 1);
}

#[test]
fn test_pool_stats_dealloc() {
    let mut stats = QPoolStats::new(10);
    stats.on_alloc();
    stats.on_dealloc();
    assert_eq!(stats.free_blocks, 10);
    assert_eq!(stats.used_blocks, 0);
}

#[test]
fn test_pool_stats_utilization() {
    let mut stats = QPoolStats::new(10);
    stats.on_alloc();
    stats.on_alloc();
    assert_eq!(stats.utilization(), 20);
}

// Test the define_event macro
define_event!(SimpleEvent, QSignal::new(100));
define_event!(DataEvent, QSignal::new(101), {
    value: u32,
    data: [u8; 8]
});

#[test]
fn test_define_event_simple() {
    let event = SimpleEvent;
    assert_eq!(event.signal(), QSignal::new(100));
}

#[test]
fn test_define_event_with_data() {
    let event = DataEvent {
        value: 42,
        data: [1, 2, 3, 4, 5, 6, 7, 8],
    };
    assert_eq!(event.signal(), QSignal::new(101));
    assert_eq!(event.value, 42);
}
