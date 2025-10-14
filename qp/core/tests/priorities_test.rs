//! Priority tests for qp-core  

use qp_core::QPriority;

#[test]
fn test_priority_creation() {
    let prio = QPriority::new(5);
    assert!(prio.is_ok());
}

#[test]
fn test_priority_invalid_zero() {
    let prio = QPriority::new(0);
    assert!(prio.is_err());
}

#[test]
fn test_priority_raw() {
    let prio = QPriority::new(10).unwrap();
    assert_eq!(prio.raw(), 10);
}

#[test]
fn test_priority_ordering() {
    let p1 = QPriority::new(5).unwrap();
    let p2 = QPriority::new(10).unwrap();
    assert!(p2 > p1);
}

#[test]
fn test_priority_constants() {
    assert!(QPriority::MIN.raw() > 0);
    assert!(QPriority::MAX.raw() > QPriority::MIN.raw());
}
