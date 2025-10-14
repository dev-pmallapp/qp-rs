#![no_std]
#![forbid(unsafe_code)]

//! # QP Event Processor (QEP)
//! 
//! Hierarchical state machine engine implementing UML statecharts.
//! Provides the core state machine execution engine with:
//! - Entry and exit actions
//! - State transitions with guards
//! - Hierarchical state nesting
//! - Event dispatch and handling

use qp_core::{QEvent, QResult, QError, QStateHandler, QStateReturn, QStateMachine};

pub mod hsm;
pub mod transition;

pub use transition::*;

#[cfg(test)]
mod tests;

/// Maximum nesting depth for hierarchical states
pub const MAX_STATE_DEPTH: usize = 8;

/// State machine execution context
pub struct QHsm {
    /// Current active state
    state: QStateHandler,
    /// Temporary state during transitions
    temp: QStateHandler,
}

impl QHsm {
    /// Create a new hierarchical state machine
    pub const fn new(initial_state: QStateHandler) -> Self {
        Self {
            state: initial_state,
            temp: initial_state,
        }
    }
    
    /// Get the current state
    pub fn state(&self) -> QStateHandler {
        self.state
    }
    
    /// Dispatch an event to the state machine
    pub fn dispatch(&mut self, event: &dyn QEvent) -> QResult<()> {
        // Save the current state
        let mut s = self.state;
        let mut r;
        
        // Process the event through the state hierarchy
        loop {
            r = (s)(self, event);
            match r {
                QStateReturn::Handled => {
                    // Event was handled
                    return Ok(());
                }
                QStateReturn::Super(parent) => {
                    // Delegate to parent state
                    s = parent;
                }
                QStateReturn::Transition(target) => {
                    // Perform state transition
                    return self.transition(target);
                }
                QStateReturn::Unhandled => {
                    // No handler found in hierarchy
                    return Ok(());
                }
                QStateReturn::Initial(_) => {
                    // Initial transition should only occur during init
                    return Err(QError::InvalidTransition);
                }
            }
        }
    }
    
    /// Execute a state transition
    fn transition(&mut self, target: QStateHandler) -> QResult<()> {
        // This is a simplified transition - full implementation would:
        // 1. Find the Least Common Ancestor (LCA) of source and target
        // 2. Exit states from source up to (but not including) LCA
        // 3. Enter states from LCA down to target
        // 4. Execute entry actions
        
        // For now, simple direct transition:
        self.state = target;
        Ok(())
    }
    
    /// Trigger the initial transition
    fn initial_transition(&mut self, target: QStateHandler) -> QResult<()> {
        self.state = target;
        Ok(())
    }
}

impl QStateMachine for QHsm {
    fn current_state(&self) -> QStateHandler {
        self.state
    }
    
    fn set_state(&mut self, state: QStateHandler) {
        self.state = state;
    }
    
    fn init(&mut self) -> QResult<()> {
        // Trigger initial transition
        let initial_evt = InitEvent;
        let r = (self.state)(self, &initial_evt);
        
        match r {
            QStateReturn::Initial(target) => {
                self.initial_transition(target)
            }
            _ => Err(QError::InvalidTransition),
        }
    }
    
    fn dispatch(&mut self, event: &dyn QEvent) -> QResult<()> {
        self.dispatch(event)
    }
}

/// Special init event for initial transitions
struct InitEvent;

impl QEvent for InitEvent {
    fn signal(&self) -> qp_core::QSignal {
        qp_core::QSignal::INIT
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QHsm {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "QHsm");
    }
}
