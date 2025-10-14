//! Clock Tick Service for POSIX
//!
//! Provides periodic clock tick generation using high-resolution timers.
//! Implements drift-free timing using monotonic clocks.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Default tick rate in Hz
const DEFAULT_TICKS_PER_SEC: u32 = 100;

/// Nanoseconds per second
const NSEC_PER_SEC: u64 = 1_000_000_000;

/// Global ticker state
static TICKER_RUNNING: AtomicBool = AtomicBool::new(false);
static TICK_RATE_HZ: AtomicU32 = AtomicU32::new(DEFAULT_TICKS_PER_SEC);
static TICKER_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static INIT: Once = Once::new();

/// Clock tick callback type
type TickCallback = fn();

/// Global tick callback
static TICK_CALLBACK: Mutex<Option<TickCallback>> = Mutex::new(None);

/// Clock tick configuration
pub struct ClockTick {
    rate_hz: u32,
}

impl ClockTick {
    /// Create a new clock tick configuration
    pub fn new(rate_hz: u32) -> Self {
        ClockTick { rate_hz }
    }
    
    /// Get the tick period as a Duration
    pub fn period(&self) -> Duration {
        Duration::from_nanos(NSEC_PER_SEC / self.rate_hz as u64)
    }
}

/// Initialize the time service
pub fn init() {
    INIT.call_once(|| {
        TICK_RATE_HZ.store(DEFAULT_TICKS_PER_SEC, Ordering::SeqCst);
    });
}

/// Set the clock tick rate
///
/// # Arguments
///
/// * `ticks_per_sec` - Number of ticks per second (Hz)
///
/// # Examples
///
/// ```
/// use qp_posix::set_tick_rate;
///
/// set_tick_rate(100); // 100 Hz tick rate
/// ```
pub fn set_tick_rate(ticks_per_sec: u32) {
    assert!(ticks_per_sec > 0, "Tick rate must be greater than 0");
    assert!(ticks_per_sec <= 10_000, "Tick rate too high (max 10kHz)");
    
    TICK_RATE_HZ.store(ticks_per_sec, Ordering::SeqCst);
}

/// Register a clock tick callback
///
/// The callback will be invoked periodically at the configured tick rate.
pub fn register_tick_callback(callback: TickCallback) {
    let mut cb = TICK_CALLBACK.lock().unwrap();
    *cb = Some(callback);
}

/// Start the ticker thread
///
/// Spawns a dedicated thread that generates periodic clock ticks at the
/// configured rate. Uses monotonic time to avoid drift.
///
/// # Examples
///
/// ```no_run
/// use qp_posix::{set_tick_rate, start_ticker, register_tick_callback};
///
/// fn on_tick() {
///     println!("Tick!");
/// }
///
/// set_tick_rate(10); // 10 Hz
/// register_tick_callback(on_tick);
/// start_ticker();
/// ```
pub fn start_ticker() {
    // Only start once
    if TICKER_RUNNING.swap(true, Ordering::SeqCst) {
        return; // Already running
    }
    
    let rate_hz = TICK_RATE_HZ.load(Ordering::SeqCst);
    let tick_period = Duration::from_nanos(NSEC_PER_SEC / rate_hz as u64);
    
    let handle = thread::spawn(move || {
        ticker_thread(tick_period);
    });
    
    let mut thread_guard = TICKER_THREAD.lock().unwrap();
    *thread_guard = Some(handle);
}

/// Stop the ticker thread
pub fn stop_ticker() {
    TICKER_RUNNING.store(false, Ordering::SeqCst);
    
    // Wait for ticker thread to finish
    let mut thread_guard = TICKER_THREAD.lock().unwrap();
    if let Some(handle) = thread_guard.take() {
        drop(thread_guard); // Release lock before joining
        let _ = handle.join();
    }
}

/// Ticker thread implementation
///
/// Uses monotonic time to avoid drift. Sleeps until the next tick time
/// (absolute) rather than sleeping for a relative duration.
fn ticker_thread(tick_period: Duration) {
    // Get initial monotonic time
    let start = Instant::now();
    let mut next_tick = start;
    
    while TICKER_RUNNING.load(Ordering::Relaxed) {
        // Advance to next tick (absolute time)
        next_tick += tick_period;
        
        // Calculate sleep duration
        let now = Instant::now();
        if next_tick > now {
            let sleep_duration = next_tick - now;
            thread::sleep(sleep_duration);
        }
        
        // Invoke tick callback
        if let Some(callback) = *TICK_CALLBACK.lock().unwrap() {
            callback();
        }
    }
}

/// Get the current tick rate in Hz
pub fn get_tick_rate() -> u32 {
    TICK_RATE_HZ.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn test_clock_tick_period() {
        let tick = ClockTick::new(100);
        assert_eq!(tick.period(), Duration::from_millis(10));
        
        let tick = ClockTick::new(1000);
        assert_eq!(tick.period(), Duration::from_micros(1000));
    }

    #[test]
    fn test_set_tick_rate() {
        set_tick_rate(50);
        assert_eq!(get_tick_rate(), 50);
        
        set_tick_rate(200);
        assert_eq!(get_tick_rate(), 200);
    }

    #[test]
    fn test_ticker_thread() {
        static TICK_COUNT: AtomicUsize = AtomicUsize::new(0);
        
        fn test_callback() {
            TICK_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        
        init();
        set_tick_rate(100); // 100 Hz = 10ms period
        register_tick_callback(test_callback);
        start_ticker();
        
        // Let it run for ~100ms
        thread::sleep(Duration::from_millis(100));
        
        stop_ticker();
        
        let count = TICK_COUNT.load(Ordering::SeqCst);
        // Should have approximately 10 ticks (100ms / 10ms)
        // Allow some tolerance for timing jitter
        assert!(count >= 8 && count <= 12, "Expected ~10 ticks, got {}", count);
    }
}
