//! UART Echo Example
//!
//! Demonstrates UART usage with the HAL.
//! Echoes back any data received on UART1.
//!
//! Build for ESP32:
//! ```
//! cargo build --features esp32s3
//! ```

#![no_std]
#![no_main]

use core::alloc::{GlobalAlloc, Layout};

// Simple bump allocator
struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    // Example demonstrating the API
    //
    // In actual usage with ESP32:
    //
    // use hal::uart::{UartPort, UartConfig};
    // use hal_esp::EspUart;
    //
    // // Create and configure UART1
    // let mut uart = EspUart::new(1).unwrap();
    // uart.configure(&UartConfig::default()).unwrap();
    //
    // let mut buffer = [0u8; 128];
    //
    // loop {
    //     // Read with 100ms timeout
    //     match uart.read(&mut buffer, 100) {
    //         Ok(n) if n > 0 => {
    //             // Echo back
    //             uart.write(&buffer[..n]).unwrap();
    //         }
    //         _ => {}
    //     }
    // }

    loop {
        // Placeholder loop
    }
}
