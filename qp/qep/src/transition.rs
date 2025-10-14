//! State transition handling

use qp_core::{QResult, QError, QStateHandler};
use crate::QHsm;

/// Transition execution path
pub struct QTransition {
    /// States to exit (from innermost to outermost)
    exit_path: [Option<QStateHandler>; 8],
    /// States to enter (from outermost to innermost)
    entry_path: [Option<QStateHandler>; 8],
    /// Number of states to exit
    exit_count: usize,
    /// Number of states to enter
    entry_count: usize,
}

impl QTransition {
    /// Create a new empty transition
    pub const fn new() -> Self {
        Self {
            exit_path: [None; 8],
            entry_path: [None; 8],
            exit_count: 0,
            entry_count: 0,
        }
    }
    
    /// Add a state to exit
    pub fn add_exit(&mut self, state: QStateHandler) -> QResult<()> {
        if self.exit_count >= self.exit_path.len() {
            return Err(QError::Framework);
        }
        self.exit_path[self.exit_count] = Some(state);
        self.exit_count += 1;
        Ok(())
    }
    
    /// Add a state to enter
    pub fn add_entry(&mut self, state: QStateHandler) -> QResult<()> {
        if self.entry_count >= self.entry_path.len() {
            return Err(QError::Framework);
        }
        self.entry_path[self.entry_count] = Some(state);
        self.entry_count += 1;
        Ok(())
    }
    
    /// Execute the transition
    pub fn execute(&self, _hsm: &mut QHsm) -> QResult<()> {
        // Execute exit actions (innermost to outermost)
        for i in 0..self.exit_count {
            if let Some(_state) = self.exit_path[i] {
                // Call exit action
                // In full implementation: state.exit()
            }
        }
        
        // Execute entry actions (outermost to innermost)  
        for i in (0..self.entry_count).rev() {
            if let Some(_state) = self.entry_path[i] {
                // Call entry action
                // In full implementation: state.entry()
            }
        }
        
        Ok(())
    }
}

impl Default for QTransition {
    fn default() -> Self {
        Self::new()
    }
}
