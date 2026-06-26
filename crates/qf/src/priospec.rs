/// Priority and preemption threshold packed into a single u16.
///
/// Encodes base priority in the low byte and preemption threshold in the high byte.
/// This matches the representation used in QP/C++ v8.x.
///
/// # Layout
/// - `[7:0]`  : Base priority of the Active Object.
/// - `[15:8]` : Preemption threshold. If `0`, threshold is implicitly set
///   to the priority itself (meaning the Active Object is fully preemptible).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QPrioSpec(pub u16);

impl QPrioSpec {
    /// Creates a new `QPrioSpec` with base priority and preemption threshold.
    pub const fn new(priority: u8, threshold: u8) -> Self {
        Self((priority as u16) | ((threshold as u16) << 8))
    }

    /// Creates a `QPrioSpec` specifying only a priority (threshold matches priority).
    pub const fn priority_only(priority: u8) -> Self {
        Self::new(priority, priority)
    }

    /// Returns the base priority.
    pub const fn priority(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    /// Returns the preemption threshold.
    ///
    /// If the encoded threshold byte is `0`, returns the priority itself.
    pub const fn threshold(self) -> u8 {
        let th = (self.0 >> 8) as u8;
        if th == 0 {
            self.priority()
        } else {
            th
        }
    }
}

/// Helper constructor matching QP/C++ `Q_PRIO()` macro.
pub const fn q_prio(priority: u8, threshold: u8) -> QPrioSpec {
    QPrioSpec::new(priority, threshold)
}
