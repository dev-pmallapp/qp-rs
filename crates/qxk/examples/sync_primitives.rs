//! Example demonstrating QXK synchronization primitives.
//!
//! This example shows how to use semaphores, mutexes, message queues,
//! and condition variables for thread coordination.

use qxk::primitives::{CondVar, MessageQueue, MutexPrim, Semaphore};
use qxk::scheduler::QxkScheduler;
use qxk::thread::ThreadId;

fn main() {
    println!("=== QXK Synchronization Primitives Demo ===\n");

    demo_semaphore();
    demo_binary_semaphore();
    demo_mutex();
    demo_message_queue();
    demo_condvar();
}

fn demo_semaphore() {
    println!("1. Semaphore Example");
    println!("   - Counting semaphore for resource management");

    let sched = QxkScheduler::new(None);
    let sem = Semaphore::new(3); // 3 resources available
    println!("   Initial count: {}", sem.count());

    // Acquire resources
    sem.try_wait();
    sem.try_wait();
    println!("   After 2 acquisitions: {}", sem.count());

    // Release a resource
    sem.signal(&sched).unwrap();
    println!("   After 1 release: {}", sem.count());

    println!("   ✓ Semaphore works correctly\n");
}

fn demo_mutex() {
    println!("2. Mutex Example");
    println!("   - Mutual exclusion for shared data protection");

    let sched = QxkScheduler::new(None);
    let mutex = MutexPrim::new();
    let thread1 = ThreadId(1);
    let thread2 = ThreadId(2);

    // Thread 1 acquires the lock
    assert!(mutex.try_lock(thread1));
    println!("   Thread 1 acquired lock");
    println!("   Owner: {:?}", mutex.owner());

    // Thread 2 cannot acquire while thread 1 holds it
    assert!(!mutex.try_lock(thread2));
    println!("   Thread 2 blocked (as expected)");

    // Thread 1 releases
    mutex.unlock(thread1, &sched).unwrap();
    println!("   Thread 1 released lock");

    // Now thread 2 can acquire
    assert!(mutex.try_lock(thread2));
    println!("   Thread 2 acquired lock");
    println!("   Owner: {:?}", mutex.owner());

    mutex.unlock(thread2, &sched).unwrap();
    println!("   ✓ Mutex works correctly\n");
}

fn demo_message_queue() {
    println!("3. Message Queue Example");
    println!("   - FIFO inter-thread communication");

    let sched = QxkScheduler::new(None);
    let queue: MessageQueue<String> = MessageQueue::new(5);

    // Send messages
    queue.try_send("Hello".to_string(), &sched).unwrap();
    queue.try_send("World".to_string(), &sched).unwrap();
    queue.try_send("from".to_string(), &sched).unwrap();
    queue.try_send("QXK".to_string(), &sched).unwrap();

    println!("   Sent 4 messages");
    println!("   Queue length: {}/{}", queue.len(), queue.capacity());

    // Receive messages in FIFO order
    println!("   Receiving messages:");
    while !queue.is_empty() {
        if let Ok(msg) = queue.try_receive() {
            println!("     - {}", msg);
        }
    }

    println!("   ✓ Message queue works correctly\n");
}

fn demo_condvar() {
    println!("4. Condition Variable Example");
    println!("   - Thread coordination via wait/notify");

    let sched = QxkScheduler::new(None);
    let cv = CondVar::new();
    let thread1 = ThreadId(10);
    let thread2 = ThreadId(11);

    println!("   Initial waiting: {}", cv.waiting_count());

    // Threads register as waiting
    let _ = cv.wait(thread1, 5, &sched);
    let _ = cv.wait(thread2, 3, &sched);
    println!("   After 2 waits: {} threads waiting", cv.waiting_count());

    // Notify one thread
    cv.notify_one(&sched);
    println!("   After notify_one: {} threads waiting", cv.waiting_count());

    // Register another waiter
    let _ = cv.wait(ThreadId(12), 7, &sched);
    println!("   After another wait: {} threads waiting", cv.waiting_count());

    // Notify all remaining
    cv.notify_all(&sched);
    println!("   After notify_all: {} threads waiting", cv.waiting_count());

    println!("   ✓ Condition variable works correctly\n");
}

fn demo_binary_semaphore() {
    println!("5. Binary Semaphore Example");
    println!("   - Acts like a signal flag");

    let sched = QxkScheduler::new(None);
    let sem = Semaphore::binary();

    // Try to wait - should fail (no signal yet)
    assert!(!sem.try_wait());
    println!("   Initial wait failed (expected)");

    // Signal
    sem.signal(&sched).unwrap();
    println!("   Sent signal (count=1)");

    // Try to signal again - should fail (already at max)
    assert!(sem.signal(&sched).is_err());
    println!("   Second signal failed (overflow protection)");

    // Now wait succeeds
    assert!(sem.try_wait());
    println!("   Wait succeeded (count=0)");

    println!("   ✓ Binary semaphore works correctly\n");
}
