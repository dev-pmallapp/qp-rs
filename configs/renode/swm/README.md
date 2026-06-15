# renode-lora-multinode

Multi-node LoRa wireless simulation and testing setup for Renode.

## Project Structure

```
renode-lora-multinode/
├── ref_platform/
│   ├── node_sx1276.repl      # STM32F4 + SX1276 platform
│   └── node_sx1262.repl      # nRF52840 + SX1262 platform
├── scripts/
│   ├── lora_multinode.resc   # Main simulation script (4 nodes)
│   └── debug_session.resc    # Interactive debug helper
├── tests/
│   └── test_lora_multinode.robot   # Robot Framework test suite
├── firmware/                 # Place your compiled ELF files here
│   ├── sensor_node.elf       # (add your sensor firmware)
│   └── gateway.elf           # (add your gateway firmware)
└── logs/                     # PCAP dumps go here (auto-created)
```

## Node Layout

```
(0,0,0)          (25,0,0)         (50,0,0)        (200,0,0)
   │                │                │                │
node-sensor-1   node-gateway    node-sensor-2   node-out-of-range
  [SX1276]        [SX1262]        [SX1276]         [SX1276]
   ◄──── 25 ────►◄──── 25 ────►
   ◄─────────── 50 ────────────►
                                                ◄── 150+ ──► (out of 100-unit range)
```

**Range limit: 100 units** — `node-out-of-range` cannot communicate with any other node.

## Quick Start

### 1. Add firmware

Place your compiled firmware ELF files in `firmware/`:
- `firmware/sensor_node.elf` — LoRa sensor firmware
- `firmware/gateway.elf`     — LoRa gateway firmware

### 2. Run simulation (interactive)

```bash
renode scripts/lora_multinode.resc
```

Or from inside the Renode monitor:
```
(monitor) include @scripts/lora_multinode.resc
```

### 3. Run automated tests

```bash
renode-test tests/test_lora_multinode.robot
```

Override firmware paths:
```bash
renode-test tests/test_lora_multinode.robot \
  --variable SENSOR_FW:path/to/sensor.elf \
  --variable GATEWAY_FW:path/to/gateway.elf
```

## Test Cases

| ID   | Name | What it validates |
|------|------|-------------------|
| TC-1 | All nodes boot | Radio init message on UART |
| TC-2 | Sensor 1 → Gateway | Basic TX/RX within range |
| TC-3 | Sensor 2 → Gateway | Basic TX/RX within range |
| TC-4 | Gateway broadcast | One TX reaches multiple RX |
| TC-5 | Out-of-range isolation | No reception beyond 100 units |
| TC-6 | Dynamic disconnect/reconnect | Hot-unplug simulation |
| TC-7 | Multi-hop routing | Gateway relays sensor-1 → sensor-2 |

## Key Renode Commands

```bash
# Switch between machines
mach set "node-sensor-1"
mach set "node-gateway"

# Check radio peripheral
sysbus.radio

# Move a node
wireless SetPosition sysbus.radio X Y Z

# Change range limit
loraMedium SetRangeWirelessFunction 200

# Disconnect a node from the medium
connector Disconnect sysbus.radio loraMedium

# Capture traffic (open in Wireshark)
emulation LogIEEE802_15_4Traffic @logs/capture.pcap loraMedium
```

## Important Notes

- Renode has no native `LoRaMedium`. The `IEEE802_15_4Medium` is used as the
  generic wireless transport — LoRa protocol behavior (SF, BW, CR) is entirely
  handled by your firmware running on the emulated CPUs.
- The SX1276 peripheral must implement `IRadio` to connect to the wireless medium.
  Verify with `peripherals` in the monitor after loading the platform.
- Replace `SPI.SX1276` / `SPI.SX1261` in the `.repl` files with the exact
  class name available in your Renode version.
- UART strings like `"LoRa radio ready"` and `"RX ..."` must match what your
  firmware actually prints — update `test_lora_multinode.robot` accordingly.
