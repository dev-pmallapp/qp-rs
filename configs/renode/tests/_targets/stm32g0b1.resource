*** Settings ***
Documentation     Per-target Robot resource for STM32G0B1CETx (ARM Cortex-M0+).
...
...               Loaded by suite files via `Resource    ${TARGET_RESOURCE}` where
...               TARGET_RESOURCE is selected at runtime by scripts/renode-test-matrix.sh
...               (or by passing --variable TARGET_RESOURCE:configs/renode/tests/_targets/stm32g0b1.resource).
...
...               Phase 4.1 of the STM32 Renode parity plan: SoC-specific bits
...               (analyzer name, firmware path, machine name, base addresses)
...               live here so suite files stay platform-agnostic.

*** Variables ***
# Project root, relative to this resource file.
${PROJECT_ROOT}             ${CURDIR}${/}..${/}..${/}..${/}..

# Identity
${TARGET_ID}                stm32g0b1
${TARGET_ARCH}              arm
${TARGET_TRIPLE}            thumbv6m-none-eabi
${TARGET_MACHINE_NAME}      SWM-G0b1-Devkit

# Renode platform `.resc` and friends
${TARGET_PLATFORM_RESC}     ${PROJECT_ROOT}${/}configs${/}renode${/}platform${/}arm${/}stm32g0b1_devkit${/}stm32g0b1_devkit.resc
# Brand-role .resc files are not yet provided per-port; suites that need
# them fall back to the generic platform resc + $bin override.
${TARGET_GAGAN_RESC}        ${TARGET_PLATFORM_RESC}
${TARGET_PRAMUKH_RESC}      ${TARGET_PLATFORM_RESC}
${TARGET_MULTINODE_RESC}    ${PROJECT_ROOT}${/}configs${/}renode${/}swm${/}swm_multinode_mixed.resc

# Console analyzer name as it appears on the sysbus.
# STM32G0B1 Nucleo routes its bring-up console through USART1 (PA9/PA10).
${TARGET_CONSOLE_ANALYZER}  usart1
${TARGET_CONSOLE_PATH}      sysbus.usart1

# Firmware ELFs per role.  Brand-name bins per Phase 3.3; built by
# `make arm TARGET=stm32g0b1 ROLE=<role>`.
${FW_DIR}                   ${PROJECT_ROOT}${/}target${/}${TARGET_TRIPLE}${/}debug
${FW_GAGAN}                 ${FW_DIR}${/}gagan
${FW_TALAB}                 ${FW_DIR}${/}talab
${FW_PRAMUKH}               ${FW_DIR}${/}pramukh
${FW_DHARA}                 ${FW_DIR}${/}dhara
${FW_NALVAR}                ${FW_DIR}${/}nalvar
${FW_GRIHA}                 ${FW_DIR}${/}griha

# Base addresses for sim-only magic registers in peripheral stubs.
# These mirror what stm32g0b1_devkit.resc registers via LoadPlatformDescriptionFromString.
${ADDR_LR1121_SPI}          0x40013000
${ADDR_BATTERY_ADC_MMV}     0x40012400
${ADDR_SOLAR_CHARGER}       0x60010000
# Ultrasonic / pressure-I2C are not wired on the G0B1 platform today
# (the dhara/nalvar bring-up bins only need the relay block); kept as
# placeholders so cross-target keywords compile.
${ADDR_ULTRASONIC}          0x00000000
${ADDR_PRESSURE_I2C}        0x00000000
${ADDR_RELAY_GPIO}          0x50000400

# Minimum PC after a clean boot (post reset-vector) — STM32G0 flash is at 0x08000000.
${BOOT_MIN_PC}              0x08000200

# GDB server port — arm ports use 3334 to avoid clashing with riscv 3333.
${GDB_PORT}                 3334
