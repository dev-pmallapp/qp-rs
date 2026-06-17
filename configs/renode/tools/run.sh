#!/usr/bin/env bash
# tools/run.sh — Convenience wrapper around Renode
#
# Usage:
#   ./tools/run.sh list                         List all available platforms
#   ./tools/run.sh run  <arch> <platform>       Launch interactive simulation
#   ./tools/run.sh test <arch> <platform>       Run Robot Framework tests
#   ./tools/run.sh test all                     Run every test suite
#   ./tools/run.sh gdb  <arch> <platform> [port] Attach GDB to running sim
#
# Examples:
#   ./tools/run.sh run  riscv esp32c6
#   ./tools/run.sh run  arm-cortex-m stm32f4
#   ./tools/run.sh test riscv esp32c6
#   ./tools/run.sh test all

set -euo pipefail

RENODE="${RENODE_PATH:-renode}"
# configs/renode/tools → configs/renode → configs → repo root
REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
ROOT="$REPO_ROOT/configs/renode"

usage() {
  sed -n '/^# Usage/,/^[^#]/p' "$0" | grep '^#' | sed 's/^# \?//'
  exit 1
}

list_platforms() {
  echo ""
  echo "RISC-V platforms:"
  for d in "$ROOT/platforms/riscv"/*/; do
    name=$(basename "$d")
    [[ "$name" == _template ]] && continue
    echo "  riscv/$name"
  done
  echo ""
  echo "ARM Cortex-M platforms:"
  for d in "$ROOT/platforms/arm-cortex-m"/*/; do
    name=$(basename "$d")
    [[ "$name" == _template ]] && continue
    echo "  arm-cortex-m/$name"
  done
  echo ""
}

run_sim() {
  local arch="$1" platform="$2"
  local script="$ROOT/scripts/${arch}/${platform}_devkit.resc"
  if [[ ! -f "$script" ]]; then
    echo "ERROR: No launch script found at $script"
    echo "       Create one by copying platforms/${arch}/_template/ and placing the resc in scripts/${arch}/"
    exit 1
  fi
  echo "Launching $arch/$platform ..."
  cd "$REPO_ROOT"
  "$RENODE" "$script"
}

run_tests() {
  local arch="${1:-all}" platform="${2:-}"

  if [[ "$arch" == "all" ]]; then
    echo "Running all test suites ..."
    cd "$ROOT"
    renode-test tests/common/smoke_all_platforms.robot
    for f in tests/riscv/*.robot tests/arm-cortex-m/*.robot; do
      [[ -f "$f" ]] && renode-test "$f"
    done
  else
    local robot="$ROOT/tests/${arch}/${platform}_tests.robot"
    if [[ ! -f "$robot" ]]; then
      echo "ERROR: No test file at $robot"
      exit 1
    fi
    echo "Running tests for $arch/$platform ..."
    cd "$ROOT"
    renode-test "$robot"
  fi
}

attach_gdb() {
  local arch="$1" platform="$2" port="${3:-3333}"
  local elf="$ROOT/firmware/${arch}/${platform}/app.elf"
  if [[ ! -f "$elf" ]]; then
    echo "WARNING: ELF not found at $elf — connecting without symbols"
    elf=""
  fi
  echo "Attaching GDB to :$port ..."
  if command -v riscv32-unknown-elf-gdb &>/dev/null && [[ "$arch" == riscv ]]; then
    riscv32-unknown-elf-gdb $elf -ex "target remote :$port"
  elif command -v gdb-multiarch &>/dev/null; then
    gdb-multiarch $elf -ex "target remote :$port"
  else
    echo "ERROR: gdb-multiarch or riscv32-unknown-elf-gdb not found"
    exit 1
  fi
}

CMD="${1:-help}"
case "$CMD" in
  list)  list_platforms ;;
  run)   [[ $# -lt 3 ]] && usage; run_sim "$2" "$3" ;;
  test)  [[ $# -lt 2 ]] && usage; run_tests "${2:-all}" "${3:-}" ;;
  gdb)   [[ $# -lt 3 ]] && usage; attach_gdb "$2" "$3" "${4:-3333}" ;;
  *)     usage ;;
esac
