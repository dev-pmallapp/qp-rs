//! Dining Philosophers Problem on Linux (POSIX)
//!
//! This example demonstrates the classic DPP concurrency problem using the QP framework.
//! Five philosophers sit at a round table with five forks between them. Each philosopher
//! alternates between thinking and eating. To eat, a philosopher needs both adjacent forks.
//!
//! This implementation uses:
//! - Active objects for each philosopher
//! - Table active object to manage fork resources
//! - Hierarchical state machines for philosopher behavior
//! - Event-driven communication between philosophers and table
//! - POSIX port with std library support
//! - QS software tracing (optional, enabled by default)
//!
//! Build without QS:
//! ```bash
//! cargo build --release --no-default-features
//! ```

use std::thread;
use std::time::Duration;

use qp_core::{QEvent, QSignal, QStateHandler, QStateReturn, QStateMachine};
use qp_qep::QHsm;

#[cfg(feature = "qs")]
use qp_qs::{self as qs, QSRecordType};

/// Number of philosophers
const N_PHILO: usize = 5;

/// Signals for the DPP application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
enum DPPSignal {
    /// Philosopher wants to eat
    Hungry = 1,
    /// Philosopher finished eating
    Done = 2,
    /// Table grants permission to eat
    Eat = 3,
    /// Timeout to transition from thinking to hungry
    Timeout = 4,
}

impl From<DPPSignal> for QSignal {
    fn from(sig: DPPSignal) -> Self {
        QSignal::new(sig as u16)
    }
}

impl DPPSignal {
    /// Get signal name for debugging/tracing
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Hungry => "HUNGRY_SIG",
            Self::Done => "DONE_SIG",
            Self::Eat => "EAT_SIG",
            Self::Timeout => "TIMEOUT_SIG",
        }
    }
}

/// Produce signal dictionary for QS tracing
#[cfg(feature = "qs")]
fn produce_sig_dict() {
    #[cfg(feature = "qs")]
    {
        use qp_posix::qs;
        
        // Send signal dictionary entries using POSIX port helper
        // Use signal value as distinguishing object pointer (common QP pattern for global signals)
        qs::send_sig_dict(DPPSignal::Hungry as u16, DPPSignal::Hungry as usize, DPPSignal::Hungry.name());
        qs::send_sig_dict(DPPSignal::Done as u16, DPPSignal::Done as usize, DPPSignal::Done.name());
        qs::send_sig_dict(DPPSignal::Eat as u16, DPPSignal::Eat as usize, DPPSignal::Eat.name());
        qs::send_sig_dict(DPPSignal::Timeout as u16, DPPSignal::Timeout as usize, DPPSignal::Timeout.name());
        qs::flush().ok();
        
        println!("[QS] Sent signal dictionary (4 entries)");
    }
    
    #[cfg(not(feature = "qs"))]
    {
        println!("[QS] Signal Dictionary:");
        println!("  {} = {}", DPPSignal::Eat as u16, DPPSignal::Eat.name());
        println!("  {} = {}", DPPSignal::Done as u16, DPPSignal::Done.name());
        println!("  {} = {}", DPPSignal::Timeout as u16, DPPSignal::Timeout.name());
        println!("  {} = {}", DPPSignal::Hungry as u16, DPPSignal::Hungry.name());
    }
}

/// Event with philosopher ID
#[derive(Debug, Clone)]
struct PhiloEvent {
    signal: QSignal,
    philo_id: u8,
}

impl PhiloEvent {
    fn new(signal: DPPSignal, philo_id: u8) -> Self {
        Self {
            signal: signal.into(),
            philo_id,
        }
    }
}

impl QEvent for PhiloEvent {
    fn signal(&self) -> QSignal {
        self.signal
    }
}

/// Simple event without data
struct SimpleEvent {
    signal: QSignal,
}

impl SimpleEvent {
    fn new(signal: DPPSignal) -> Self {
        Self {
            signal: signal.into(),
        }
    }
}

impl QEvent for SimpleEvent {
    fn signal(&self) -> QSignal {
        self.signal
    }
}

/// Philosopher active object
struct Philosopher {
    hsm: QHsm,
    id: u8,
}

impl Philosopher {
    const fn new(id: u8) -> Self {
        Self {
            hsm: QHsm::new(Self::top),
            id,
        }
    }

    /// Get current state name for logging
    fn state_name(&self) -> &'static str {
        let state_fn = self.current_state();
        if state_fn as usize == Self::thinking as usize {
            "THINKING"
        } else if state_fn as usize == Self::hungry as usize {
            "HUNGRY"
        } else if state_fn as usize == Self::eating as usize {
            "EATING"
        } else {
            "UNKNOWN"
        }
    }

    /// Thinking state
    fn thinking(me: &mut dyn QStateMachine, e: &dyn QEvent) -> QStateReturn {
        let sig_val = e.signal().0;
        if sig_val == DPPSignal::Timeout as u16 {
            // Trace the transition
            #[cfg(feature = "qs")]
            qs::qs_sm_tran!(me, Self::thinking as usize, Self::hungry as usize);
            
            // Transition to hungry
            QStateReturn::Transition(Self::hungry)
        } else {
            QStateReturn::Super(Self::top)
        }
    }

    /// Hungry state - waiting for forks
    fn hungry(me: &mut dyn QStateMachine, e: &dyn QEvent) -> QStateReturn {
        let sig_val = e.signal().0;
        if sig_val == DPPSignal::Eat as u16 {
            // Trace the transition
            #[cfg(feature = "qs")]
            qs::qs_sm_tran!(me, Self::hungry as usize, Self::eating as usize);
            
            // Transition to eating
            QStateReturn::Transition(Self::eating)
        } else {
            QStateReturn::Super(Self::top)
        }
    }

    /// Eating state
    fn eating(me: &mut dyn QStateMachine, e: &dyn QEvent) -> QStateReturn {
        let sig_val = e.signal().0;
        if sig_val == DPPSignal::Timeout as u16 {
            // Trace the transition
            #[cfg(feature = "qs")]
            qs::qs_sm_tran!(me, Self::eating as usize, Self::thinking as usize);
            
            // Transition to thinking
            QStateReturn::Transition(Self::thinking)
        } else {
            QStateReturn::Super(Self::top)
        }
    }

    /// Top state
    fn top(_me: &mut dyn QStateMachine, _e: &dyn QEvent) -> QStateReturn {
        QStateReturn::Handled
    }
}

impl QStateMachine for Philosopher {
    fn current_state(&self) -> QStateHandler {
        self.hsm.state()
    }

    fn set_state(&mut self, state: QStateHandler) {
        self.hsm.set_state(state);
    }
}

/// Table active object - manages fork allocation
struct Table {
    /// Fork availability (true = available)
    forks: [bool; N_PHILO],
    /// Philosopher states (true = eating)
    is_eating: [bool; N_PHILO],
}

impl Table {
    const fn new() -> Self {
        Self {
            forks: [true; N_PHILO],
            is_eating: [false; N_PHILO],
        }
    }

    /// Check if philosopher can eat (both forks available)
    fn can_eat(&self, n: usize) -> bool {
        let left = n;
        let right = (n + 1) % N_PHILO;
        self.forks[left] && self.forks[right] && !self.is_eating[n]
    }

    /// Allocate forks to philosopher
    fn allocate_forks(&mut self, n: usize) {
        let left = n;
        let right = (n + 1) % N_PHILO;
        self.forks[left] = false;
        self.forks[right] = false;
        self.is_eating[n] = true;
    }

    /// Free forks from philosopher
    fn free_forks(&mut self, n: usize) {
        let left = n;
        let right = (n + 1) % N_PHILO;
        self.forks[left] = true;
        self.forks[right] = true;
        self.is_eating[n] = false;
    }

    /// Handle hungry event
    fn on_hungry(&mut self, philo_id: u8) -> bool {
        let n = philo_id as usize;
        if n < N_PHILO && self.can_eat(n) {
            self.allocate_forks(n);
            true
        } else {
            false
        }
    }

    /// Handle done event
    fn on_done(&mut self, philo_id: u8) {
        let n = philo_id as usize;
        if n < N_PHILO {
            self.free_forks(n);
        }
    }
}

fn main() {
    println!("\n╔════════════════════════════════════════╗");
    println!("║  QP Framework - Dining Philosophers    ║");
    println!("║  Running on Linux (POSIX)              ║");
    #[cfg(feature = "qs")]
    println!("║  QS Tracing: ENABLED                   ║");
    #[cfg(not(feature = "qs"))]
    println!("║  QS Tracing: DISABLED                  ║");
    println!("╚════════════════════════════════════════╝\n");

    // Initialize QS tracing
    #[cfg(feature = "qs")]
    {
        use qp_posix::qs;
        
        // Initialize TCP output to QSpy host tool (standard QP port)
        match qs::init_tcp("127.0.0.1", 6601) {
            Ok(_) => println!("QS: Initialized TCP output to QSpy at 127.0.0.1:6601"),
            Err(e) => {
                eprintln!("QS: Failed to initialize TCP: {}", e);
                eprintln!("QS: Make sure QSpy is running in another terminal:");
                eprintln!("    cd tools/qspy && cargo run --release");
                std::process::exit(1);
            }
        }
        
        // Send TARGET_INFO record first
        qs::send_target_info("QP-Rust 8.1.0", "DPP-Linux-Rust");
        
        // Send signal dictionary entries
        println!("QS: Sending signal dictionaries...");
        produce_sig_dict();
        
        println!("QS: Flushing signal dictionaries...");
        match qs::flush() {
            Ok(_) => println!("QS: Signal dictionaries flushed successfully"),
            Err(e) => eprintln!("QS: Failed to flush signal dictionaries: {}", e),
        }
        
        // Send an initial test trace
        println!("QS: Sending test trace record...");
        if qs::begin(unsafe { std::mem::transmute(0u8) }, 0) {
            qs::str("DPP_STARTED");
            qs::u32(0xDEADBEEF);
            qs::end();
        }
        match qs::flush() {
            Ok(_) => println!("QS: Test trace flushed successfully"),
            Err(e) => eprintln!("QS: Failed to flush test trace: {}", e),
        }
        println!();
    }
    
    // Initialize QP POSIX port
    qp_posix::init();

    // Set tick rate to 10 Hz for demonstration
    qp_posix::set_tick_rate(10);

    // Create philosophers
    let mut philos: [Philosopher; N_PHILO] = [
        Philosopher { hsm: QHsm::new(Philosopher::top), id: 0 },
        Philosopher { hsm: QHsm::new(Philosopher::top), id: 1 },
        Philosopher { hsm: QHsm::new(Philosopher::top), id: 2 },
        Philosopher { hsm: QHsm::new(Philosopher::top), id: 3 },
        Philosopher { hsm: QHsm::new(Philosopher::top), id: 4 },
    ];

    // Create table
    let mut table = Table::new();

    // Initialize philosophers - set them all to thinking state
    for i in 0..N_PHILO {
        philos[i].set_state(Philosopher::thinking);
        println!("Philosopher {} initialized in THINKING state", i);
        
        // Trace SM initialization using proper format
        #[cfg(feature = "qs")]
        {
            qp_qs::qs_sm_init!(
                &philos[i] as *const _ as usize,
                Philosopher::thinking as usize,
                Philosopher::thinking as usize
            );
            qs::flush().ok();
        }
    }

    println!("\n╔════════════════════════════════════════╗");
    println!("║  Simulation Starting...                ║");
    println!("║  Press Ctrl-C to stop                  ║");
    println!("╚════════════════════════════════════════╝\n");

    let mut cycle = 0u32;
    let mut philo_idx = 0usize;
    let mut eating_count = [0u32; N_PHILO];
    let mut think_time = [0u32; N_PHILO];  // Time spent thinking
    let mut eat_time = [0u32; N_PHILO];     // Time spent eating

    // Main simulation loop
    loop {
        cycle += 1;

        // Each philosopher gets a time slice
        let philo = &mut philos[philo_idx];
        let state_fn = philo.current_state();

        // Determine philosopher's state and act accordingly
        if state_fn as usize == Philosopher::thinking as usize {
            think_time[philo_idx] += 1;
            
            // Philosopher is thinking, gets hungry after some time
            if think_time[philo_idx] >= 50 + (philo_idx as u32 * 10) {
                println!("[{}] Philosopher {} thinking -> HUNGRY (thought for {} cycles)", 
                    cycle, philo_idx, think_time[philo_idx]);
                
                // Trace the transition (QS_SM_TRAN = 7)
                #[cfg(feature = "qs")]
                {
                    if qs::begin(unsafe { std::mem::transmute(7u8) }, philo_idx as u8) {
                        qs::u8(philo_idx as u8);  // philosopher ID
                        qs::str("thinking");       // source state
                        qs::str("hungry");         // target state
                        qs::u32(think_time[philo_idx]); // think cycles
                        qs::end();
                    }
                    qs::flush().ok(); // Flush immediately for real-time tracing
                }
                
                philo.hsm.set_state(Philosopher::hungry);
                think_time[philo_idx] = 0;
                
                // Request forks from table
                if table.on_hungry(philo_idx as u8) {
                    println!("[{}] Philosopher {} got forks -> EATING", cycle, philo_idx);
                    
                    // Trace the transition (QS_SM_TRAN = 7)
                    #[cfg(feature = "qs")]
                    {
                        if qs::begin(unsafe { std::mem::transmute(7u8) }, philo_idx as u8) {
                            qs::u8(philo_idx as u8);  // philosopher ID
                            qs::str("hungry");         // source state
                            qs::str("eating");         // target state
                            qs::u32(cycle);            // cycle number
                            qs::end();
                        }
                        qs::flush().ok(); // Flush immediately for real-time tracing
                    }
                    
                    philo.hsm.set_state(Philosopher::eating);
                    eating_count[philo_idx] += 1;
                } else {
                    println!("[{}] Philosopher {} waiting for forks...", cycle, philo_idx);
                }
            }
        } else if state_fn as usize == Philosopher::hungry as usize {
            // Try to get forks again each cycle
            if table.on_hungry(philo_idx as u8) {
                println!("[{}] Philosopher {} got forks -> EATING", cycle, philo_idx);
                philo.hsm.set_state(Philosopher::eating);
                eating_count[philo_idx] += 1;
            }
        } else if state_fn as usize == Philosopher::eating as usize {
            eat_time[philo_idx] += 1;
            
            // Philosopher is eating, finishes after some time
            if eat_time[philo_idx] >= 30 + (philo_idx as u32 * 5) {
                println!("[{}] Philosopher {} eating -> DONE (ate for {} cycles)", 
                    cycle, philo_idx, eat_time[philo_idx]);
                
                // Trace the transition (QS_SM_TRAN = 7)
                #[cfg(feature = "qs")]
                {
                    if qs::begin(unsafe { std::mem::transmute(7u8) }, philo_idx as u8) {
                        qs::u8(philo_idx as u8);  // philosopher ID
                        qs::str("eating");         // source state
                        qs::str("thinking");       // target state
                        qs::u32(eat_time[philo_idx]); // eat cycles
                        qs::end();
                    }
                    qs::flush().ok(); // Flush immediately for real-time tracing
                }
                
                // Release forks
                table.on_done(philo_idx as u8);
                eat_time[philo_idx] = 0;
                
                philo.hsm.set_state(Philosopher::thinking);
                
                println!("[{}] Philosopher {} released forks -> THINKING", cycle, philo_idx);
            }
        }

        // Move to next philosopher (round-robin)
        philo_idx = (philo_idx + 1) % N_PHILO;

        // Print status and flush QS trace periodically
        if cycle % 100 == 0 {
            println!("\n╔════════ Status at cycle {} ════════╗", cycle);
            println!("║ Eating:      [{} {} {} {} {}]",
                if table.is_eating[0] { "0" } else { "-" },
                if table.is_eating[1] { "1" } else { "-" },
                if table.is_eating[2] { "2" } else { "-" },
                if table.is_eating[3] { "3" } else { "-" },
                if table.is_eating[4] { "4" } else { "-" });
            println!("║ Forks:       [{} {} {} {} {}]",
                if table.forks[0] { "✓" } else { "✗" },
                if table.forks[1] { "✓" } else { "✗" },
                if table.forks[2] { "✓" } else { "✗" },
                if table.forks[3] { "✓" } else { "✗" },
                if table.forks[4] { "✓" } else { "✗" });
            println!("║ Eat count:   [{:2} {:2} {:2} {:2} {:2}]",
                eating_count[0], eating_count[1], eating_count[2],
                eating_count[3], eating_count[4]);
            println!("╚═════════════════════════════════════════╝\n");
            
            // Flush QS trace buffer
            #[cfg(feature = "qs")]
            if let Err(e) = qs::flush() {
                eprintln!("Warning: Failed to flush QS trace: {}", e);
            }
        }

        // Delay between cycles
        thread::sleep(Duration::from_millis(50));
    }
}
