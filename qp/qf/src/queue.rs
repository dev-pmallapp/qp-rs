//! Event queue implementation for active objects

use crate::{QEvent, QEventRef, QResult, QError, QSignal};
use heapless::Deque;
use core::marker::PhantomData;

/// Event queue for active objects
/// 
/// A bounded FIFO queue that stores event signals. The actual event data
/// is managed separately through event pools.
pub struct QEventQueue<const N: usize> {
    queue: Deque<QSignal, N>,
}

impl<const N: usize> QEventQueue<N> {
    /// Create a new empty event queue
    pub const fn new() -> Self {
        Self {
            queue: Deque::new(),
        }
    }
    
    /// Post an event to the back of the queue (FIFO)
    pub fn post(&mut self, event: &dyn QEvent) -> QResult<()> {
        self.queue
            .push_back(event.signal())
            .map_err(|_| QError::QueueFull)
    }
    
    /// Post an event to the front of the queue (LIFO/high priority)
    pub fn post_lifo(&mut self, event: &dyn QEvent) -> QResult<()> {
        self.queue
            .push_front(event.signal())
            .map_err(|_| QError::QueueFull)
    }
    
    /// Get the next event signal from the queue
    pub fn get_signal(&mut self) -> Option<QSignal> {
        self.queue.pop_front()
    }
    
    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
    
    /// Check if the queue is full
    pub fn is_full(&self) -> bool {
        self.queue.is_full()
    }
    
    /// Get the number of events in the queue
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    
    /// Get the maximum capacity of the queue
    pub const fn capacity(&self) -> usize {
        N
    }
    
    /// Clear all events from the queue
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

impl<const N: usize> Default for QEventQueue<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Extended event queue that stores full event references
/// 
/// This is useful when you need to store the actual event data in the queue
/// rather than just the signal. It has higher memory overhead but provides
/// more flexibility.
pub struct QEventRefQueue<'a, const N: usize> {
    queue: Deque<QEventRef<'a>, N>,
    _phantom: PhantomData<&'a ()>,
}

impl<'a, const N: usize> QEventRefQueue<'a, N> {
    /// Create a new empty event reference queue
    pub const fn new() -> Self {
        Self {
            queue: Deque::new(),
            _phantom: PhantomData,
        }
    }
    
    /// Post an event reference to the queue
    pub fn post(&mut self, event: QEventRef<'a>) -> QResult<()> {
        self.queue
            .push_back(event)
            .map_err(|_| QError::QueueFull)
    }
    
    /// Get the next event reference from the queue
    pub fn get(&mut self) -> Option<QEventRef<'a>> {
        self.queue.pop_front()
    }
    
    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
    
    /// Get the number of events in the queue
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    
    /// Get the maximum capacity of the queue
    pub const fn capacity(&self) -> usize {
        N
    }
}

impl<'a, const N: usize> Default for QEventRefQueue<'a, N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QStaticEvent;
    
    #[test]
    fn test_event_queue_fifo() {
        let mut queue: QEventQueue<4> = QEventQueue::new();
        
        let evt1 = QStaticEvent::new(QSignal::new(10));
        let evt2 = QStaticEvent::new(QSignal::new(20));
        let evt3 = QStaticEvent::new(QSignal::new(30));
        
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
        
        queue.post(&evt1).unwrap();
        queue.post(&evt2).unwrap();
        queue.post(&evt3).unwrap();
        
        assert_eq!(queue.len(), 3);
        assert!(!queue.is_empty());
        
        assert_eq!(queue.get_signal(), Some(QSignal::new(10)));
        assert_eq!(queue.get_signal(), Some(QSignal::new(20)));
        assert_eq!(queue.get_signal(), Some(QSignal::new(30)));
        assert_eq!(queue.get_signal(), None);
        
        assert!(queue.is_empty());
    }
    
    #[test]
    fn test_event_queue_lifo() {
        let mut queue: QEventQueue<4> = QEventQueue::new();
        
        let evt1 = QStaticEvent::new(QSignal::new(10));
        let evt2 = QStaticEvent::new(QSignal::new(20));
        
        queue.post(&evt1).unwrap();
        queue.post_lifo(&evt2).unwrap(); // Posted to front
        
        assert_eq!(queue.get_signal(), Some(QSignal::new(20))); // LIFO event comes first
        assert_eq!(queue.get_signal(), Some(QSignal::new(10)));
    }
    
    #[test]
    fn test_event_queue_full() {
        let mut queue: QEventQueue<2> = QEventQueue::new();
        
        let evt1 = QStaticEvent::new(QSignal::new(10));
        let evt2 = QStaticEvent::new(QSignal::new(20));
        let evt3 = QStaticEvent::new(QSignal::new(30));
        
        assert!(queue.post(&evt1).is_ok());
        assert!(queue.post(&evt2).is_ok());
        
        // Queue is full
        assert!(queue.is_full());
        assert_eq!(queue.post(&evt3), Err(QError::QueueFull));
    }
}
