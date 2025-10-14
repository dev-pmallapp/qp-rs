//! Active object tests for qp-qf

use qp_core::{QPriority};

#[test]
fn test_priority_in_qf() {
    let p1 = QPriority::new(5).unwrap();
    let p2 = QPriority::new(10).unwrap();
    assert_ne!(p1, p2);
}

// More tests will be added as the QF implementation progresses
#[test]
fn test_placeholder() {
    // Placeholder test to ensure test infrastructure works
    assert_eq!(2 + 2, 4);
}
