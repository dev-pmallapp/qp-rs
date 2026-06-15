.PHONY: all build check test clean help
.PHONY: hal-check hal-check-cmsis hal-check-lxsis hal-check-rvsis
.PHONY: $(filter example-% run-% flash-%,$(.TARGETS))

CARGO       ?= cargo
CARGO_FLAGS ?=

# Cross-compilation targets for embedded boards
TARGET_esp32s3 := --target xtensa-esp32s3-espidf
TARGET_esp32c6 := --target riscv32imac-esp-espidf
TARGET_host    :=

all: build

build:
	$(CARGO) build $(CARGO_FLAGS)

check:
	$(CARGO) check $(CARGO_FLAGS)

test:
	$(CARGO) test $(CARGO_FLAGS)

clean:
	$(CARGO) clean

# ─── HAL sub-workspace checks ─────────────────────────────────────────────────
# The hal/ directory is an independent workspace (excluded from root Cargo.toml).
# Use these targets to verify each *SIS crate without requiring cross-compilers.

hal-check:
	cd hal && $(CARGO) check -p hal-cmsis -p hal-lxsis -p hal-rvsis

hal-check-cmsis:
	cd hal && $(CARGO) check -p hal-cmsis --features stm32f4xx
	cd hal && $(CARGO) check -p hal-cmsis --features nrf52840
	cd hal && $(CARGO) check -p hal-cmsis --features lpc1768

hal-check-lxsis:
	cd hal && $(CARGO) check -p hal-lxsis --features esp32
	cd hal && $(CARGO) check -p hal-lxsis --features esp32s2
	cd hal && $(CARGO) check -p hal-lxsis --features esp32s3

hal-check-rvsis:
	cd hal && $(CARGO) check -p hal-rvsis --features esp32c6
	cd hal && $(CARGO) check -p hal-rvsis --features esp32c3
	cd hal && $(CARGO) check -p hal-rvsis --features gd32vf103

# ─── Pattern rules ────────────────────────────────────────────────────────────
# make example-<board>-<name>   build an example for a board
# make run-<board>-<name>       run an example  (host only)
# make flash-<board>-<name>     flash to device (embedded only)
#
# Boards:    host  esp32s3  esp32c6
# Examples:  dpp   lora_send

_board = $(firstword $(subst -, ,$*))
_name  = $(patsubst $(_board)-%,%,$*)

example-%:
	$(CARGO) build -p $(_name) \
	    --no-default-features --features $(_board) \
	    $(TARGET_$(_board)) $(CARGO_FLAGS)

run-%:
	$(if $(filter-out host,$(_board)),$(error run-$* requires board=host),)
	$(CARGO) run -p $(_name) \
	    --no-default-features --features $(_board) $(CARGO_FLAGS)

flash-%:
	$(if $(filter host,$(_board)),$(error flash-$* is not supported for board=host),)
	$(CARGO) espflash flash -p $(_name) \
	    --no-default-features --features $(_board) \
	    $(TARGET_$(_board)) $(CARGO_FLAGS)

# ─── Help ─────────────────────────────────────────────────────────────────────
help:
	@printf 'Usage:\n'
	@printf '  make                           build workspace (host default features)\n'
	@printf '  make build                     build workspace\n'
	@printf '  make check                     cargo check\n'
	@printf '  make test                      cargo test\n'
	@printf '  make clean                     clean build artifacts\n'
	@printf '\n'
	@printf '  make hal-check                 cargo check all HAL crates\n'
	@printf '  make hal-check-cmsis           check hal-cmsis with stm32f4xx/nrf52840/lpc1768\n'
	@printf '  make hal-check-lxsis           check hal-lxsis with esp32/esp32s2/esp32s3\n'
	@printf '  make hal-check-rvsis           check hal-rvsis with esp32c6/esp32c3/gd32vf\n'
	@printf '\n'
	@printf '  make example-<board>-<name>    build example for board\n'
	@printf '  make run-<board>-<name>        run example  (board=host only)\n'
	@printf '  make flash-<board>-<name>      flash to device (embedded boards only)\n'
	@printf '\n'
	@printf 'Boards:    host  esp32s3  esp32c6\n'
	@printf 'Examples:  dpp   lora_send\n'
	@printf '\n'
	@printf 'Examples:\n'
	@printf '  make example-host-dpp\n'
	@printf '  make example-esp32s3-dpp\n'
	@printf '  make example-esp32c6-dpp\n'
	@printf '  make example-host-lora_send\n'
	@printf '  make example-esp32c6-lora_send\n'
	@printf '  make run-host-dpp\n'
	@printf '  make flash-esp32c6-lora_send\n'
	@printf '\n'
	@printf 'Override cargo flags:\n'
	@printf '  CARGO_FLAGS=--release make example-host-dpp\n'
