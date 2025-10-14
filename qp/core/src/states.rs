//! State machine types and state handling for the QP framework

use crate::{QEvent, QResult};

/// State machine return codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QStateReturn {
    /// Event was handled in this state
    Handled,
    /// Event was not handled, try parent state
    Unhandled, 
    /// Transition to a new state
    Transition(QStateHandler),
    /// Transition to parent state
    Super(QStateHandler),
    /// Initial transition (used in initial pseudo-state)
    Initial(QStateHandler),
}

impl QStateReturn {
    /// Check if the event was handled
    pub fn is_handled(&self) -> bool {
        matches!(self, QStateReturn::Handled | QStateReturn::Transition(_) | QStateReturn::Initial(_))
    }
    
    /// Check if this is a transition
    pub fn is_transition(&self) -> bool {
        matches!(self, QStateReturn::Transition(_) | QStateReturn::Initial(_))
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QStateReturn {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            QStateReturn::Handled => defmt::write!(fmt, "Handled"),
            QStateReturn::Unhandled => defmt::write!(fmt, "Unhandled"),
            QStateReturn::Transition(_) => defmt::write!(fmt, "Transition"),
            QStateReturn::Super(_) => defmt::write!(fmt, "Super"),
            QStateReturn::Initial(_) => defmt::write!(fmt, "Initial"),
        }
    }
}

/// State handler function pointer type
pub type QStateHandler = fn(&mut dyn QStateMachine, &dyn QEvent) -> QStateReturn;

/// Trait for hierarchical state machines
pub trait QStateMachine {
    /// Get the current state handler
    fn current_state(&self) -> QStateHandler;
    
    /// Set the current state handler  
    fn set_state(&mut self, state: QStateHandler);
    
    /// Initialize the state machine
    fn init(&mut self) -> QResult<()> where Self: Sized {
        // Trigger initial transition
        let init_event = crate::QStaticEvent::new(crate::QSignal::INIT);
        self.dispatch(&init_event)
    }
    
    /// Dispatch an event to the state machine
    fn dispatch(&mut self, event: &dyn QEvent) -> QResult<()> where Self: Sized {
        let mut current = self.current_state();
        
        // Walk up the state hierarchy until event is handled
        loop {
            match current(self, event) {
                QStateReturn::Handled => break,
                QStateReturn::Transition(target) => {
                    self.transition(target, event)?;
                    break;
                }
                QStateReturn::Initial(target) => {
                    self.set_state(target);
                    // Execute entry action for initial target
                    let entry_event = crate::QStaticEvent::new(crate::QSignal::ENTRY);
                    let _ = target(self, &entry_event);
                    break;
                }
                QStateReturn::Super(parent) => {
                    current = parent;
                    continue;
                }
                QStateReturn::Unhandled => {
                    // Event not handled by any state in hierarchy
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    /// Execute state transition with entry/exit actions
    fn transition(&mut self, target: QStateHandler, _event: &dyn QEvent) -> QResult<()> where Self: Sized {
        // Execute exit action on current state
        let current = self.current_state();
        let exit_event = crate::QStaticEvent::new(crate::QSignal::EXIT);
        let _ = current(self, &exit_event);
        
        // Change to target state
        self.set_state(target);
        
        // Execute entry action on target state  
        let entry_event = crate::QStaticEvent::new(crate::QSignal::ENTRY);
        let _ = target(self, &entry_event);
        
        Ok(())
    }
}

/// Macro to help define state handler functions
#[macro_export]
macro_rules! state_handler {
    ($name:ident, $me:ident: $me_type:ty, $event:ident: $event_type:ty, $body:block) => {
        fn $name($me: &mut dyn $crate::QStateMachine, $event: &dyn $crate::QEvent) -> $crate::QStateReturn {
            // Downcast to concrete type
            let $me = $me.downcast_mut::<$me_type>().expect("Invalid state machine type");
            
            // Handle the event
            match $event.signal() {
                $crate::QSignal::ENTRY => {
                    // State entry action
                    $crate::QStateReturn::Handled
                }
                $crate::QSignal::EXIT => {
                    // State exit action  
                    $crate::QStateReturn::Handled
                }
                _ => $body
            }
        }
    };
}

/// Helper trait for downcasting state machines
pub trait QStateMachineDowncast {
    fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T>;
    fn downcast_ref<T: 'static>(&self) -> Option<&T>;
}

impl<T: 'static> QStateMachineDowncast for T {
    fn downcast_mut<U: 'static>(&mut self) -> Option<&mut U> {
        // This is a simplified version - in a real implementation,
        // you'd use proper Any trait downcasting
        None
    }
    
    fn downcast_ref<U: 'static>(&self) -> Option<&U> {
        // This is a simplified version - in a real implementation,  
        // you'd use proper Any trait downcasting
        None
    }
}

/// Convenient return values for state handlers
pub const HANDLED: QStateReturn = QStateReturn::Handled;
pub const UNHANDLED: QStateReturn = QStateReturn::Unhandled;

/// Create a transition return value
pub const fn transition(target: QStateHandler) -> QStateReturn {
    QStateReturn::Transition(target)
}

/// Create a super state return value  
pub const fn super_state(parent: QStateHandler) -> QStateReturn {
    QStateReturn::Super(parent)
}

/// Create an initial transition return value
pub const fn initial(target: QStateHandler) -> QStateReturn {
    QStateReturn::Initial(target)
}