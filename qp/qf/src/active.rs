//! Active object trait and base implementation

use crate::{QEvent, QEventRef, QPriority, QResult, QStateMachine};
use core::fmt;

/// Trait for active objects in the QP framework
/// 
/// Active objects are encapsulated, event-driven concurrent objects that:
/// - Have their own event queue
/// - Execute in their own thread of control  
/// - Communicate via asynchronous message passing
/// - Implement hierarchical state machines
pub trait QActive: QStateMachine + Send {
    /// Get the priority of this active object
    fn priority(&self) -> QPriority;
    
    /// Post an event to this active object's queue
    /// 
    /// Returns Ok(()) if event was successfully posted, or Err(QError::QueueFull)
    /// if the queue is full.
    fn post(&mut self, event: &dyn QEvent) -> QResult<()>;
    
    /// Post an event to the front of the queue (high priority)
    fn post_lifo(&mut self, event: &dyn QEvent) -> QResult<()>;
    
    /// Try to get the next event from the queue (non-blocking)
    fn get(&mut self) -> Option<QEventRef<'_>>;
    
    /// Check if the event queue is empty
    fn is_empty(&self) -> bool;
    
    /// Get the number of events in the queue
    fn queue_len(&self) -> usize;
    
    /// Get the maximum queue capacity
    fn queue_capacity(&self) -> usize;
    
    /// Initialize the active object (called during system initialization)
    /// This is separate from QStateMachine::init to allow dynamic dispatch
    fn initialize(&mut self) -> QResult<()>;
    
    /// Stop the active object (cleanup before shutdown)
    fn stop(&mut self) -> QResult<()> {
        Ok(())
    }
}

/// Active object lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QActiveState {
    /// Active object is being initialized
    Init,
    /// Active object is ready to process events
    Ready,
    /// Active object is running and processing events
    Running,
    /// Active object is stopped
    Stopped,
}

#[cfg(feature = "defmt")]
impl defmt::Format for QActiveState {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            QActiveState::Init => defmt::write!(fmt, "Init"),
            QActiveState::Ready => defmt::write!(fmt, "Ready"),
            QActiveState::Running => defmt::write!(fmt, "Running"),
            QActiveState::Stopped => defmt::write!(fmt, "Stopped"),
        }
    }
}

impl fmt::Display for QActiveState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QActiveState::Init => write!(f, "Init"),
            QActiveState::Ready => write!(f, "Ready"),
            QActiveState::Running => write!(f, "Running"),
            QActiveState::Stopped => write!(f, "Stopped"),
        }
    }
}

/// Base structure for implementing active objects
/// 
/// This provides the common infrastructure needed by all active objects,
/// including priority, lifecycle state, and event queue management.
pub struct QActiveBase {
    priority: QPriority,
    state: QActiveState,
}

impl QActiveBase {
    /// Create a new active object base with the given priority
    pub const fn new(priority: QPriority) -> Self {
        Self {
            priority,
            state: QActiveState::Init,
        }
    }
    
    /// Get the current lifecycle state
    pub fn state(&self) -> QActiveState {
        self.state
    }
    
    /// Set the lifecycle state
    pub fn set_state(&mut self, state: QActiveState) {
        self.state = state;
    }
    
    /// Get the priority
    pub fn priority(&self) -> QPriority {
        self.priority
    }
}

/// Macro to help implement the QActive trait for custom active objects
#[macro_export]
macro_rules! impl_active_object {
    ($name:ty, $priority:expr, $queue_capacity:expr) => {
        impl $crate::QActive for $name {
            fn priority(&self) -> $crate::QPriority {
                self.base.priority()
            }
            
            fn post(&mut self, event: &dyn $crate::QEvent) -> $crate::QResult<()> {
                self.queue.post(event)
            }
            
            fn post_lifo(&mut self, event: &dyn $crate::QEvent) -> $crate::QResult<()> {
                self.queue.post_lifo(event)
            }
            
            fn get(&mut self) -> Option<$crate::QEventRef<'_>> {
                self.queue.get()
            }
            
            fn is_empty(&self) -> bool {
                self.queue.is_empty()
            }
            
            fn queue_len(&self) -> usize {
                self.queue.len()
            }
            
            fn queue_capacity(&self) -> usize {
                $queue_capacity
            }
        }
    };
}
