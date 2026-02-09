# ESP32-S3 Port (WIP)

This crate hosts the in-progress ESP32-S3 port for the Quantum Platform
kernels (`qf`, `qk`, `qs`). The initial milestone focuses on capturing the
expected integration points:

- scheduler critical-section mapping for the Xtensa LX7 core
- hardware timer provisioning to back `TimeEvent` ticks
- QS trace transport over the ESP32 high-speed USB CDC or UART peripheral
- optional Wi-Fi/Bluetooth coexistence hooks so radio ISRs cooperate with QK

## Next Steps

1. Prototype the low-level interrupt entry/exit wrappers using `xtensa_lx_rt`.
2. Select a GPTimer instance to generate a periodic tick and expose an ISR shim
   that calls into `QkKernel::tick()`.
3. Define a safe abstraction around the ESP-IDF heap/allocator for dynamic
   events when `alloc` is enabled.
4. Implement a QS backend that streams records over USB CDC while the radio is
   active.

Contributions are welcome—please open an issue to coordinate work before
submitting pull requests.
