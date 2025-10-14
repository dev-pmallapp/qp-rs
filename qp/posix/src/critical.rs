//! Critical Section Management for POSIX
//!
//! Provides thread-safe critical sections using standard library mutexes.
//! Unlike embedded targets, POSIX systems use actual OS mutexes rather than
//! interrupt disabling.

use std::sync::{Mutex, MutexGuard};
use std::cell::Cell;

/// Global critical section mutex
static CRITICAL_SECTION: Mutex<()> = Mutex::new(());

thread_local! {
    /// Thread-local nesting counter for critical sections
    static NESTING_LEVEL: Cell<usize> = const { Cell::new(0) };
}

/// RAII guard for critical section
///
/// Automatically exits the critical section when dropped.
pub struct CriticalSection {
    _guard: MutexGuard<'static, ()>,
}

impl CriticalSection {
    /// Enter a critical section (internal use)
    fn enter() -> Self {
        let nesting = NESTING_LEVEL.with(|n| {
            let current = n.get();
            n.set(current + 1);
            current
        });
        
        // Only lock the mutex on first entry (no recursive locking)
        assert_eq!(nesting, 0, "Critical sections must not nest in QP POSIX port");
        
        let guard = CRITICAL_SECTION.lock().unwrap();
        
        CriticalSection {
            _guard: unsafe { std::mem::transmute(guard) },
        }
    }
}

impl Drop for CriticalSection {
    fn drop(&mut self) {
        NESTING_LEVEL.with(|n| {
            let current = n.get();
            assert!(current > 0, "Critical section underflow");
            n.set(current - 1);
        });
        // MutexGuard drop will unlock automatically
    }
}

/// Enter a critical section
///
/// Returns a guard that will automatically exit the critical section when dropped.
///
/// # Examples
///
/// ```
/// use qp_posix::enter_critical;
///
/// let _guard = enter_critical();
/// // Critical section code here
/// // Automatically exits when _guard goes out of scope
/// ```
#[inline]
pub fn enter_critical() -> CriticalSection {
    CriticalSection::enter()
}

/// Exit a critical section
///
/// This is typically handled automatically by dropping the guard returned from
/// `enter_critical()`, but can be called explicitly if needed.
#[inline]
pub fn exit_critical(_guard: CriticalSection) {
    // Drop the guard explicitly
    drop(_guard);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_critical_section_basic() {
        let _guard = enter_critical();
        // Critical section active
        drop(_guard);
        // Critical section exited
    }

    #[test]
    fn test_critical_section_raii() {
        {
            let _guard = enter_critical();
            // Critical section active
        } // Automatically exits here
    }

    #[test]
    fn test_critical_section_mutual_exclusion() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let counter = Arc::clone(&counter);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    let _guard = enter_critical();
                    let current = counter.load(Ordering::Relaxed);
                    thread::sleep(std::time::Duration::from_micros(1));
                    counter.store(current + 1, Ordering::Relaxed);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::Relaxed), 1000);
    }

    #[test]
    #[should_panic(expected = "Critical sections must not nest")]
    fn test_no_nesting() {
        let _guard1 = enter_critical();
        let _guard2 = enter_critical(); // Should panic
    }
}
