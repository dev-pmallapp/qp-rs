//! Active object lifecycle management

use crate::{QActive, QResult};

/// Active object lifecycle operations
pub trait QLifecycle {
    /// Initialize the active object and prepare it for operation
    fn init(&mut self) -> QResult<()>;
    
    /// Start the active object's event processing
    fn start(&mut self) -> QResult<()>;
    
    /// Stop the active object and cleanup resources
    fn stop(&mut self) -> QResult<()>;
    
    /// Pause event processing (can be resumed later)
    fn pause(&mut self) -> QResult<()> {
        Ok(())
    }
    
    /// Resume event processing after pause
    fn resume(&mut self) -> QResult<()> {
        Ok(())
    }
}

/// System-wide lifecycle management
pub struct QFramework;

impl QFramework {
    /// Initialize the QP framework
    pub fn init() -> QResult<()> {
        // Framework initialization
        // - Initialize event pools
        // - Setup timing services
        // - Initialize tracing if enabled
        Ok(())
    }
    
    /// Start all registered active objects
    pub fn start_all() -> QResult<()> {
        crate::with_registry(|registry| {
            for active in registry.iter_mut() {
                active.initialize()?;
            }
            Ok(())
        })
    }
    
    /// Stop all registered active objects
    pub fn stop_all() -> QResult<()> {
        crate::with_registry(|registry| {
            for active in registry.iter_mut() {
                active.stop()?;
            }
            Ok(())
        })
    }
    
    /// Gracefully shutdown the framework
    pub fn shutdown() -> QResult<()> {
        Self::stop_all()?;
        // Additional cleanup as needed
        Ok(())
    }
}

/// Trait for implementing graceful shutdown handlers
pub trait QShutdownHandler {
    /// Called before the framework shuts down
    fn on_shutdown(&mut self) -> QResult<()> {
        Ok(())
    }
}

/// Helper function to start a single active object
pub fn start_active(active: &mut dyn QActive) -> QResult<()> {
    active.initialize()
}

/// Helper function to stop a single active object  
pub fn stop_active(active: &mut dyn QActive) -> QResult<()> {
    active.stop()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_framework_init() {
        assert!(QFramework::init().is_ok());
    }
}
