//! Active object registry for system-wide access

use crate::{QActive, QPriority, QError, QResult};
use core::cell::RefCell;
use critical_section::Mutex;

/// Maximum number of active objects that can be registered
pub const MAX_ACTIVE: usize = 32;

/// Global registry of active objects
/// 
/// The registry allows active objects to be accessed by priority level
/// throughout the system. This is used by the kernel for scheduling and
/// event delivery.
pub struct QActiveRegistry {
    /// Array of optional active object references indexed by priority
    active_objects: [Option<&'static mut dyn QActive>; MAX_ACTIVE],
}

impl QActiveRegistry {
    /// Create a new empty registry
    pub const fn new() -> Self {
        const NONE: Option<&'static mut dyn QActive> = None;
        Self {
            active_objects: [NONE; MAX_ACTIVE],
        }
    }
    
    /// Register an active object at its priority level
    /// 
    /// Returns an error if an active object is already registered at this priority
    pub fn register(&mut self, active: &'static mut dyn QActive) -> QResult<()> {
        let priority = active.priority().raw() as usize;
        
        if priority == 0 || priority >= MAX_ACTIVE {
            return Err(QError::InvalidPriority);
        }
        
        if self.active_objects[priority].is_some() {
            return Err(QError::Framework); // Priority already in use
        }
        
        self.active_objects[priority] = Some(active);
        Ok(())
    }
    
    /// Unregister an active object by priority
    pub fn unregister(&mut self, priority: QPriority) -> QResult<()> {
        let priority = priority.raw() as usize;
        
        if priority == 0 || priority >= MAX_ACTIVE {
            return Err(QError::InvalidPriority);
        }
        
        self.active_objects[priority] = None;
        Ok(())
    }
    
    /// Get an active object by priority
    pub fn get(&self, priority: QPriority) -> Option<&dyn QActive> {
        let priority = priority.raw() as usize;
        if priority >= MAX_ACTIVE {
            return None;
        }
        self.active_objects[priority].as_deref()
    }
    
    /// Get a mutable reference to an active object by priority
    pub fn get_mut(&mut self, priority: QPriority) -> Option<&mut &'static mut dyn QActive> {
        let idx = priority.raw() as usize;
        if idx >= MAX_ACTIVE {
            return None;
        }
        self.active_objects[idx].as_mut()
    }
    
    /// Iterate over all registered active objects
    pub fn iter(&self) -> impl Iterator<Item = &dyn QActive> + '_ {
        self.active_objects.iter().filter_map(|opt| opt.as_deref())
    }
    
    /// Iterate mutably over all registered active objects
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut &'static mut dyn QActive> + '_ {
        self.active_objects.iter_mut().filter_map(|opt| opt.as_mut())
    }
    
    // TODO: Implement get_next_ready with proper lifetime management
    // This will be added when implementing the kernel scheduler
}

/// Global static registry instance
static REGISTRY: Mutex<RefCell<QActiveRegistry>> = 
    Mutex::new(RefCell::new(QActiveRegistry::new()));

/// Get access to the global active object registry
pub fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&mut QActiveRegistry) -> R,
{
    critical_section::with(|cs| {
        let mut registry = REGISTRY.borrow_ref_mut(cs);
        f(&mut registry)
    })
}

/// Register an active object in the global registry
pub fn register_active(active: &'static mut dyn QActive) -> QResult<()> {
    with_registry(|registry| registry.register(active))
}

/// Unregister an active object from the global registry
pub fn unregister_active(priority: QPriority) -> QResult<()> {
    with_registry(|registry| registry.unregister(priority))
}

/// Publish an event to all registered active objects
pub fn publish(event: &dyn crate::QEvent) -> QResult<()> {
    with_registry(|registry| {
        for active in registry.iter_mut() {
            // Ignore queue full errors when publishing - best effort delivery
            let _ = active.post(event);
        }
        Ok(())
    })
}

/// Publish an event to a specific subset of active objects
pub fn publish_to<F>(event: &dyn crate::QEvent, filter: F) -> QResult<()>
where
    F: Fn(QPriority) -> bool,
{
    with_registry(|registry| {
        for active in registry.iter_mut() {
            if filter(active.priority()) {
                let _ = active.post(event);
            }
        }
        Ok(())
    })
}
