//! Producer-Consumer example demonstrating QXK thread coordination.
//!
//! This example shows how extended threads can use blocking primitives
//! (semaphores and message queues) to coordinate work. The scheduler
//! properly blocks and unblocks threads as they wait on synchronization
//! primitives.

use qxk::primitives::{MessageQueue, Semaphore};
use qxk::thread::{ThreadAction, ThreadConfig, ThreadId, ThreadPriority};
use qxk::QxkKernel;

fn main() {
    println!("=== QXK Producer-Consumer Example ===\n");

    // Create synchronization primitives
    let empty_slots = Semaphore::new(5); // 5 empty slots
    let full_slots = Semaphore::new(0); // 0 full slots initially
    let queue: MessageQueue<usize> = MessageQueue::new(5);

    // Clone primitives for sharing between threads
    let prod_empty = empty_slots.clone();
    let prod_full = full_slots.clone();
    let prod_queue = queue.clone();

    let cons_empty = empty_slots.clone();
    let cons_full = full_slots.clone();
    let cons_queue = queue.clone();

    // Producer thread
    let producer = ThreadConfig::new(
        ThreadId(1),
        ThreadPriority(5),
        Box::new(move |ctx| {
            static mut ITEM_COUNTER: usize = 0;

            // Wait for an empty slot
            match prod_empty.wait(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
                Ok(()) => {
                    // Produce an item
                    unsafe { ITEM_COUNTER += 1 };
                    let item = unsafe { ITEM_COUNTER };

                    // Send to queue (non-blocking since we have a slot)
                    match prod_queue.try_send(item, ctx.scheduler()) {
                        Ok(()) => {
                            println!("Producer: Created item #{}", item);
                            // Signal that there's a full slot
                            let _ = prod_full.signal(ctx.scheduler());

                            // Produce 10 items then terminate
                            if item < 10 {
                                ThreadAction::Continue
                            } else {
                                println!("Producer: Finished (produced {} items)", item);
                                ThreadAction::Terminated
                            }
                        }
                        Err(e) => {
                            eprintln!("Producer: Queue send failed: {}", e);
                            ThreadAction::Terminated
                        }
                    }
                }
                Err(_) => {
                    // Would block - scheduler already blocked us
                    ThreadAction::Blocked
                }
            }
        }),
    );

    // Consumer thread
    let consumer = ThreadConfig::new(
        ThreadId(2),
        ThreadPriority(4),
        Box::new(move |ctx| {
            static mut CONSUMED_COUNT: usize = 0;

            // Wait for a full slot
            match cons_full.wait(ctx.thread_id(), ctx.priority().0, ctx.scheduler()) {
                Ok(()) => {
                    // Receive from queue
                    match cons_queue.try_receive() {
                        Ok(item) => {
                            unsafe { CONSUMED_COUNT += 1 };
                            let count = unsafe { CONSUMED_COUNT };
                            println!("Consumer: Received item #{}", item);

                            // Signal that there's an empty slot
                            let _ = cons_empty.signal(ctx.scheduler());

                            // Consume 10 items then terminate
                            if count < 10 {
                                ThreadAction::Continue
                            } else {
                                println!("Consumer: Finished (consumed {} items)", count);
                                ThreadAction::Terminated
                            }
                        }
                        Err(e) => {
                            eprintln!("Consumer: Queue receive failed: {}", e);
                            ThreadAction::Terminated
                        }
                    }
                }
                Err(_) => {
                    // Would block - scheduler already blocked us
                    ThreadAction::Blocked
                }
            }
        }),
    );

    // Build and run kernel
    let mut kernel = QxkKernel::builder()
        .register_thread(producer)
        .expect("register producer")
        .register_thread(consumer)
        .expect("register consumer")
        .build()
        .expect("build kernel");

    kernel.start();
    kernel.run_until_idle();

    println!("\n✓ Producer-Consumer example completed successfully!");
}
