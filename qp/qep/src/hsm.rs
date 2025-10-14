//! Hierarchical state machine implementation details

use qp_core::{QEvent, QResult, QStateHandler, QStateReturn};
use crate::{QHsm, MAX_STATE_DEPTH};

impl QHsm {
    /// Get the path from a state to the top of the hierarchy
    pub(crate) fn get_state_path(&mut self, mut state: QStateHandler, path: &mut [QStateHandler; MAX_STATE_DEPTH]) -> usize {
        let mut depth = 0;
        path[depth] = state;
        depth += 1;
        
        // Traverse up to find all parent states
        while depth < MAX_STATE_DEPTH {
            // Use a special empty event to query for super state
            let evt = EmptyEvent;
            let r = (state)(self, &evt);
            
            match r {
                QStateReturn::Super(parent) => {
                    path[depth] = parent;
                    depth += 1;
                    state = parent;
                }
                _ => break,
            }
        }
        
        depth
    }
    
    /// Find the Least Common Ancestor of two states
    pub(crate) fn find_lca(&mut self, source: QStateHandler, target: QStateHandler) -> Option<QStateHandler> {
        let mut source_path = [source; MAX_STATE_DEPTH];
        let mut target_path = [target; MAX_STATE_DEPTH];
        
        let source_depth = self.get_state_path(source, &mut source_path);
        let target_depth = self.get_state_path(target, &mut target_path);
        
        // Find the common ancestor
        for i in (0..source_depth).rev() {
            for j in (0..target_depth).rev() {
                // Function pointer comparison - note this is for structure, not identity
                if source_path[i] as usize == target_path[j] as usize {
                    return Some(source_path[i]);
                }
            }
        }
        
        None
    }
}

/// Empty event for hierarchy traversal
struct EmptyEvent;

impl QEvent for EmptyEvent {
    fn signal(&self) -> qp_core::QSignal {
        qp_core::QSignal::EMPTY
    }
}
