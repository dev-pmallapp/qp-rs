//! Functional-safety fault model — failure-assertion programming.
//!
//! This is **Phase 1** of the [FuSa roadmap](../../../docs/FUSA.md): a single,
//! well-defined path for every detected fault, mirroring QP/C's
//! `Q_ASSERT` / `Q_onError()` design.
//!
//! # Crash-only model
//!
//! Rather than attempting complex recovery, qp-rs follows the **crash-only**
//! model recommended by the QP/C++ functional-safety viewpoint: on a detected
//! fault the framework records the fault location and halts gracefully. A
//! halted system is in a known, predictable state — safe to recover from with
//! an external watchdog/reset — whereas a system limping along after corrupt
//! internal state is not.
//!
//! # Assertion vocabulary
//!
//! Faults are classified by *who* is at fault, which is the key diagnostic
//! information in a safety case:
//!
//! - [`q_require!`] — **precondition**: the *caller* violated the contract.
//! - [`q_ensure!`] — **postcondition**: the *callee* failed to deliver.
//! - [`q_invariant!`] — **invariant**: internal data integrity is broken
//!   (e.g. an error-detecting code mismatch).
//! - [`q_assert!`] — general assertion where the precise classification does
//!   not matter.
//! - [`q_error!`] — an unconditional, unreachable-path fault.
//!
//! Each macro identifies the fault by `module_path!()` + `line!()`, giving
//! forward/backward traceability from a captured fault back to source without
//! any manual `Q_DEFINE_THIS_MODULE` bookkeeping.
//!
//! Unlike [`debug_assert!`], these assertions are **always enabled**, including
//! release builds — functional-safety integrity must not depend on debug
//! configuration.
//!
//! # Overriding the handler
//!
//! The default handler panics under `std` (test-friendly) and busy-halts under
//! `no_std`. A port should install its own handler with [`set_error_handler`]
//! to perform a platform-appropriate safe stop: disable interrupts, flush a QS
//! fault record over the trace transport, then reset via the watchdog.
//!
//! ```
//! use qf::fusa;
//!
//! fn my_safe_stop(module: &'static str, id: u32) -> ! {
//!     // emit QS fault record, kick off watchdog reset, …
//!     panic!("safe stop @ {module}:{id}");
//! }
//!
//! fusa::set_error_handler(my_safe_stop);
//! ```

/// A functional-safety fault handler.
///
/// Receives the originating module path and a fault id (the source line by
/// default) and **must not return** — it terminates in a safe stop.
pub type ErrorHandler = fn(module: &'static str, id: u32) -> !;

/// Installed handler, or `None` to fall back to [`default_on_error`].
///
/// Stored directly (no integer round-trip) so the function pointer keeps its
/// provenance. `spin::Mutex::new` is `const`, so this lives in `static` storage
/// with no runtime init in both `std` and `no_std`.
static HANDLER: spin::Mutex<Option<ErrorHandler>> = spin::Mutex::new(None);

/// Installs the functional-safety fault handler.
///
/// Replaces any previously installed handler. Typically called once during
/// port/runtime initialisation.
pub fn set_error_handler(handler: ErrorHandler) {
    *HANDLER.lock() = Some(handler);
}

/// Clears any installed handler, restoring the built-in default.
pub fn clear_error_handler() {
    *HANDLER.lock() = None;
}

/// Routes a detected fault to the installed handler, or the default.
///
/// This is the single choke point every assertion macro funnels through.
/// It never returns.
#[cold]
#[inline(never)]
pub fn on_error(module: &'static str, id: u32) -> ! {
    // Copy the handler out and release the lock *before* calling it, so a
    // handler that itself faults cannot deadlock on re-entry.
    let handler = *HANDLER.lock();
    if let Some(handler) = handler {
        handler(module, id);
    }
    default_on_error(module, id)
}

/// Built-in fault handler used when no port handler is installed.
///
/// `std`: panics with the fault location (useful in tests and on the host).
/// `no_std`: busy-halts — a port **should** install a real safe-stop handler.
#[cold]
#[inline(never)]
fn default_on_error(module: &'static str, id: u32) -> ! {
    #[cfg(feature = "std")]
    {
        panic!("qp-rs FuSa fault: {module}:{id}");
    }
    #[cfg(not(feature = "std"))]
    {
        let _ = (module, id);
        loop {
            core::hint::spin_loop();
        }
    }
}

// ── Assertion macros ──────────────────────────────────────────────────────────
//
// Defined here (not in lib.rs with the HSM macros) to keep the fault model
// self-contained. All are `#[macro_export]` so they appear at the crate root,
// matching the `q_tran!` / `qm_*` convention.

/// Asserts a **precondition** — the caller violated this function's contract.
///
/// On failure, routes `module_path!()` + `line!()` (or an explicit id) through
/// [`on_error`], which does not return.
///
/// ```
/// # use qf::q_require;
/// fn set_priority(p: u8) {
///     q_require!(p > 0); // priority 0 is reserved for the idle thread
///     // …
/// }
/// ```
#[macro_export]
macro_rules! q_require {
    ($cond:expr) => {
        if !($cond) {
            $crate::fusa::on_error(::core::module_path!(), ::core::line!());
        }
    };
    ($cond:expr, $id:expr) => {
        if !($cond) {
            $crate::fusa::on_error(::core::module_path!(), $id);
        }
    };
}

/// Asserts a **postcondition** — this function failed to deliver its contract.
#[macro_export]
macro_rules! q_ensure {
    ($cond:expr) => {
        if !($cond) {
            $crate::fusa::on_error(::core::module_path!(), ::core::line!());
        }
    };
    ($cond:expr, $id:expr) => {
        if !($cond) {
            $crate::fusa::on_error(::core::module_path!(), $id);
        }
    };
}

/// Asserts a data-integrity **invariant** — e.g. an error-detecting-code check.
#[macro_export]
macro_rules! q_invariant {
    ($cond:expr) => {
        if !($cond) {
            $crate::fusa::on_error(::core::module_path!(), ::core::line!());
        }
    };
    ($cond:expr, $id:expr) => {
        if !($cond) {
            $crate::fusa::on_error(::core::module_path!(), $id);
        }
    };
}

/// General assertion where the pre/post/invariant classification is immaterial.
#[macro_export]
macro_rules! q_assert {
    ($cond:expr) => {
        if !($cond) {
            $crate::fusa::on_error(::core::module_path!(), ::core::line!());
        }
    };
    ($cond:expr, $id:expr) => {
        if !($cond) {
            $crate::fusa::on_error(::core::module_path!(), $id);
        }
    };
}

/// Unconditionally raises a fault — marks an unreachable / forbidden path.
///
/// ```
/// # use qf::q_error;
/// # let signal = 0u16;
/// match signal {
///     0 => { /* … */ }
///     _ => q_error!(), // signal space is exhaustively handled above
/// }
/// ```
#[macro_export]
macro_rules! q_error {
    () => {
        $crate::fusa::on_error(::core::module_path!(), ::core::line!())
    };
    ($id:expr) => {
        $crate::fusa::on_error(::core::module_path!(), $id)
    };
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Passing conditions never reach [`on_error`], so this is independent of
    /// the globally-installed handler and safe to run in parallel.
    #[test]
    fn passing_assertions_do_not_fault() {
        q_require!(1 + 1 == 2);
        q_ensure!(true);
        q_invariant!(true, 42);
        q_assert!(2 > 1);
    }

    fn panic_message(err: &(dyn std::any::Any + Send)) -> String {
        err.downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    /// The fault-path cases all read/write the global `HANDLER`, so they are
    /// driven from a single sequential test to avoid racing each other.
    #[test]
    fn fault_paths() {
        static HIT: AtomicBool = AtomicBool::new(false);

        // Quiet the default panic hook for the duration — these panics are expected.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));

        // 1. Default handler: failing assertion panics with the fault location.
        clear_error_handler();
        let err = std::panic::catch_unwind(|| q_require!(false)).unwrap_err();
        assert!(panic_message(&*err).contains("FuSa fault"));

        // 2. Default handler: `q_error!` is an unconditional fault.
        let err = std::panic::catch_unwind(|| q_error!()).unwrap_err();
        assert!(panic_message(&*err).contains("FuSa fault"));

        // 3. An installed handler is invoked instead of the default.
        fn handler(_m: &'static str, _id: u32) -> ! {
            HIT.store(true, Ordering::SeqCst);
            panic!("custom handler");
        }
        set_error_handler(handler);
        let err = std::panic::catch_unwind(|| q_assert!(false)).unwrap_err();
        assert!(panic_message(&*err).contains("custom handler"));
        assert!(HIT.load(Ordering::SeqCst));

        clear_error_handler();
        std::panic::set_hook(prev);
    }
}
