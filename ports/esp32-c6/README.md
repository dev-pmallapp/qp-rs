# ESP32-C6 Port (WIP)

In-progress ESP32-C6 (RISC-V) port for [qp-rs](../../README.md). It targets the
same QP HAL contract as the other ports — tick source, QS trace transport, and
critical-section/interrupt control — for the RISC-V core.

This port backs the `esp32c6` feature of the `dpp` and `lora_send` examples:

```bash
cargo build --bin dpp-esp32-c6 --features esp32c6 --no-default-features
cargo build --bin lora_send_c6 --features esp32c6 --no-default-features
```

## Integration points

- RISC-V interrupt priority configuration and critical-section mapping
- a GPTimer instance generating the periodic tick that calls into `tick()`
- QS trace transport over UART/USB-CDC
- (for `lora_send`) real SX127x/SX126x radio over SPI via the `comms` stack

Contributions are welcome — please open an issue to coordinate before submitting
pull requests.
