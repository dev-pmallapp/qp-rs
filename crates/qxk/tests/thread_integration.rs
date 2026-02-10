//! Integration tests for QXK extended threads with blocking primitives.

use qxk::primitives::{MessageQueue, Semaphore, SyncError};
use qxk::thread::{ThreadAction, ThreadConfig, ThreadId, ThreadPriority};
use qxk::QxkKernel;

#[test]
fn thread_executes_and_terminates() {
    use std::sync::{Arc, Mutex};

    let executed = Arc::new(Mutex::new(false));
    let executed_clone = executed.clone();

    let thread = ThreadConfig::new(
        ThreadId(1),
        ThreadPriority(5),
        Box::new(move |ctx| {
            *executed_clone.lock().unwrap() = true;
            if ctx.iteration() < 3 {
                ThreadAction::Continue
            } else {
                ThreadAction::Terminated
            }
        }),
    );

    let mut kernel = QxkKernel::builder()
        .register_thread(thread)
        .expect("register thread")
        .build()
        .expect("build kernel");

    kernel.start();
    kernel.run_until_idle();

    // Thread should have executed
    assert!(*executed.lock().unwrap());
}

#[test]
fn thread_blocks_on_semaphore() {
    let sem = Semaphore::new(0); // No signals available
    let sem_clone = sem.clone();

    let thread = ThreadConfig::new(
        ThreadId(1),
        ThreadPriority(5),
        Box::new(move |ctx| {
            static mut ATTEMPTS: usize = 0;

            unsafe { ATTEMPTS += 1 };

            match sem_clone.wait(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
                Ok(()) => {
                    // Got the semaphore, terminate
                    ThreadAction::Terminated
                }
                Err(SyncError::WouldBlock) => {
                    // Blocked - this is expected first time
                    ThreadAction::Blocked
                }
                Err(e) => panic!("Unexpected error: {}", e),
            }
        }),
    );

    let mut kernel = QxkKernel::builder()
        .register_thread(thread)
        .expect("register thread")
        .build()
        .expect("build kernel");

    kernel.start();

    // First dispatch: thread tries to wait, blocks
    assert!(kernel.dispatch_once());

    // No more work (thread is blocked)
    assert!(!kernel.dispatch_once());

    // Signal the semaphore
    sem.signal(kernel.scheduler().as_ref()).unwrap();

    // Thread should be unblocked and run again
    assert!(kernel.dispatch_once());
}

#[test]
fn multiple_threads_coordinate_via_semaphore() {
    use std::sync::{Arc, Mutex};

    let sem = Semaphore::new(1); // One resource
    let sem1 = sem.clone();
    let sem2 = sem.clone();

    let acquired = Arc::new(Mutex::new(false));
    let acquired_clone = acquired.clone();

    let thread1 = ThreadConfig::new(
        ThreadId(1),
        ThreadPriority(10),
        Box::new(move |ctx| {
            if *acquired_clone.lock().unwrap() {
                return ThreadAction::Terminated;
            }

            match sem1.wait(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
                Ok(()) => {
                    *acquired_clone.lock().unwrap() = true;
                    // Hold the semaphore, don't release
                    ThreadAction::Continue
                }
                Err(SyncError::WouldBlock) => ThreadAction::Blocked,
                Err(e) => panic!("{}", e),
            }
        }),
    );

    let thread2 = ThreadConfig::new(
        ThreadId(2),
        ThreadPriority(5),
        Box::new(move |ctx| {
            match sem2.wait(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
                Ok(()) => ThreadAction::Terminated,
                Err(SyncError::WouldBlock) => ThreadAction::Blocked,
                Err(e) => panic!("{}", e),
            }
        }),
    );

    let mut kernel = QxkKernel::builder()
        .register_thread(thread1)
        .expect("register thread1")
        .register_thread(thread2)
        .expect("register thread2")
        .build()
        .expect("build kernel");

    kernel.start();

    // Thread1 (higher priority) acquires semaphore
    assert!(kernel.dispatch_once());

    // Thread1 continues holding semaphore
    assert!(kernel.dispatch_once());

    // Thread2 tries to acquire, blocks
    assert!(kernel.dispatch_once());

    // No more work (thread2 is blocked)
    assert!(!kernel.dispatch_once());
}

#[test]
fn message_queue_blocks_receiver() {
    use std::sync::{Arc, Mutex};

    let queue: MessageQueue<u32> = MessageQueue::new(5);
    let queue_clone = queue.clone();

    let received = Arc::new(Mutex::new(None));
    let received_clone = received.clone();

    let receiver = ThreadConfig::new(
        ThreadId(1),
        ThreadPriority(5),
        Box::new(move |ctx| {
            if received_clone.lock().unwrap().is_some() {
                return ThreadAction::Terminated;
            }

            match queue_clone.receive(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
                Ok(value) => {
                    *received_clone.lock().unwrap() = Some(value);
                    assert_eq!(value, 42);
                    ThreadAction::Terminated
                }
                Err(SyncError::WouldBlock) => ThreadAction::Blocked,
                Err(e) => panic!("{}", e),
            }
        }),
    );

    let mut kernel = QxkKernel::builder()
        .register_thread(receiver)
        .expect("register receiver")
        .build()
        .expect("build kernel");

    kernel.start();

    // Receiver tries to receive from empty queue, blocks
    assert!(kernel.dispatch_once());
    assert!(!kernel.dispatch_once()); // Blocked

    // Send a message
    queue.try_send(42, kernel.scheduler().as_ref()).unwrap();

    // Receiver unblocks and receives
    assert!(kernel.dispatch_once());
}

#[test]
fn thread_yield_gives_cpu_to_others() {
    let yield_count = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let yield1 = yield_count.clone();
    let yield2 = yield_count.clone();

    let thread1 = ThreadConfig::new(
        ThreadId(1),
        ThreadPriority(10),
        Box::new(move |ctx| {
            *yield1.lock().unwrap() += 1;
            if ctx.iteration() < 5 {
                ThreadAction::Yield // Yield to thread2
            } else {
                ThreadAction::Terminated
            }
        }),
    );

    let thread2 = ThreadConfig::new(
        ThreadId(2),
        ThreadPriority(5),
        Box::new(move |ctx| {
            *yield2.lock().unwrap() += 1;
            if ctx.iteration() < 5 {
                ThreadAction::Yield
            } else {
                ThreadAction::Terminated
            }
        }),
    );

    let mut kernel = QxkKernel::builder()
        .register_thread(thread1)
        .expect("register thread1")
        .register_thread(thread2)
        .expect("register thread2")
        .build()
        .expect("build kernel");

    kernel.start();
    kernel.run_until_idle();

    // Both threads should have yielded multiple times
    let count = *yield_count.lock().unwrap();
    assert!(count >= 10, "Expected at least 10 yields, got {}", count);
}
