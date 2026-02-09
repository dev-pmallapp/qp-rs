//! GPIO Blink Example
//!
//! Demonstrates portable GPIO usage across multiple platforms using HAL traits.
//!
//! Build for different platforms using feature flags:
//! - `cargo run --features esp32s3`
//! - `cargo run --features stm32f4`
//! - `cargo run --features msp432`

#![no_std]
#![no_main]

// This is a demonstration example showing the API structure.
// Actual implementations would be added to vendor HAL crates.

use core::alloc::{GlobalAlloc, Layout};

// Simple bump allocator for demonstration
struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

// Panic handler for no_std
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    // Example: Create a GPIO pin (vendor-specific initialization)
    // In real usage, you'd get this from the vendor HAL:
    //
    // #[cfg(feature = "esp32s3")]
    // let mut led = hal_esp::EspGpioPin::new(2).unwrap();
    //
    // #[cfg(feature = "stm32f4")]
    // let mut led = hal_cmsis::CmsisGpioPin::new(13).unwrap();
    //
    // Then use the portable trait interface:
    // led.set_mode(PinMode::Output).unwrap();
    //
    // loop {
    //     led.toggle().unwrap();
    //     delay_ms(500);
    // }

    loop {
        // Placeholder loop
    }
}
