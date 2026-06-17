*** Settings ***
Documentation     Per-target Robot resource for ESP32-C6 (RISC-V).
...
...               Loaded by suite files via `Resource    ${TARGET_RESOURCE}` where
...               TARGET_RESOURCE is selected at runtime by scripts/renode-test-matrix.sh
...               (or by passing --variable TARGET_RESOURCE:configs/renode/tests/_targets/esp32c6.resource).
...
...               Phase 4.1 of the STM32 Renode parity plan: SoC-specific bits
...               (analyzer name, firmware path, machine name, base addresses)
...               live here so suite files stay platform-agnostic.

*** Variables ***
# Project root, relative to this resource file.
${PROJECT_ROOT}             ${CURDIR}${/}..${/}..${/}..${/}..

# Identity
${TARGET_ID}                esp32c6
${TARGET_ARCH}              riscv
${TARGET_TRIPLE}            riscv32imac-unknown-none-elf
${TARGET_MACHINE_NAME}      SWM-Esp32c6-Lr1121

# Renode platform `.resc` and friends
${TARGET_PLATFORM_RESC}     ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}riscv${/}esp32c6_lr1121${/}esp32c6_lr1121.resc
${TARGET_GAGAN_RESC}        ${PROJECT_ROOT}${/}configs${/}renode${/}swm${/}gagan.resc
${TARGET_PRAMUKH_RESC}      ${PROJECT_ROOT}${/}configs${/}renode${/}swm${/}pramukh.resc
${TARGET_MULTINODE_RESC}    ${PROJECT_ROOT}${/}configs${/}renode${/}swm${/}swm_multinode_lr1121.resc

# Console analyzer name as it appears on the sysbus.
# ESP32-C6 routes defmt/QS-trace through the USB-Serial-JTAG block.
${TARGET_CONSOLE_ANALYZER}  usb_serial_jtag
${TARGET_CONSOLE_PATH}      sysbus.usb_serial_jtag

# Firmware ELFs per role.  Brand-name bins per Phase 3.3; built by
# `make esp ROLE=<role>` (esp32c6 prefix is `swm-<role>-esp32c6`).
${FW_DIR}                   ${PROJECT_ROOT}${/}target${/}${TARGET_TRIPLE}${/}debug
${FW_GAGAN}                 ${FW_DIR}${/}swm-gagan-esp32c6
${FW_TALAB}                 ${FW_DIR}${/}swm-talab-esp32c6
${FW_PRAMUKH}               ${FW_DIR}${/}swm-pramukh-esp32c6
${FW_DHARA}                 ${FW_DIR}${/}swm-dhara-esp32c6
${FW_NALVAR}                ${FW_DIR}${/}swm-nalvar-esp32c6
${FW_GRIHA}                 ${FW_DIR}${/}swm-griha-esp32c6

# Base addresses for sim-only magic registers in peripheral stubs.
# The Robot keywords use these to inject sensor values or read relay state.
${ADDR_LR1121_SPI}          0x60081000
${ADDR_BATTERY_ADC_MMV}     0x60090000
${ADDR_SOLAR_CHARGER}       0x60010000
${ADDR_ULTRASONIC}          0x600B0000
${ADDR_PRESSURE_I2C}        0x60023000
${ADDR_RELAY_GPIO}          0x60091000

# Minimum PC after a clean boot (post reset-vector); a smoke-level check
# that the firmware has actually advanced past startup.
${BOOT_MIN_PC}              0x40800000

# GDB server port — riscv ports use 3333 to avoid clashing with the
# arm ports' 3334.
${GDB_PORT}                 3333
