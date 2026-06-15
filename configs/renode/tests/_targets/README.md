# Per-target Robot resources (Phase 4.1)

Each `<target>.resource` file collects the SoC-specific Robot variables —
analyzer name, machine name, firmware paths, base addresses, platform
`.resc` path — so suite files in `configs/renode/tests/{smoke,integration,…}`
can stay platform-agnostic.

## Available targets

| Target ID  | Architecture | Resource file              |
|------------|--------------|----------------------------|
| `esp32c6`  | RISC-V       | `esp32c6.resource`         |
| `stm32wle5`| ARM v7E-M    | `stm32wle5.resource`       |
| `stm32g0b1`| ARM v6-M     | `stm32g0b1.resource`       |

## Use from a suite file

```robotframework
*** Variables ***
${TARGET_RESOURCE}    ${CURDIR}/../_targets/esp32c6.resource

*** Settings ***
Resource    ${RENODEKEYWORDS}
Resource    ${TARGET_RESOURCE}
```

Override at the command line so the same suite runs on a different SoC:

```sh
renode-test \
  --variable TARGET_RESOURCE:configs/renode/tests/_targets/stm32wle5.resource \
  configs/renode/tests/smoke/parametric_boot.robot
```

`scripts/renode-test-matrix.sh` (Phase 4.2) wraps this so you can write
`./scripts/renode-test-matrix.sh stm32wle5 smoke/parametric_boot.robot`.

## What goes here, what doesn't

In:
- Names that change per SoC (analyzer / machine / `usartN`).
- Filesystem paths to platform `.resc` files and firmware ELFs.
- Magic-register base addresses for the per-target peripheral stubs.
- Reset-vector / boot-min-PC thresholds.

Out:
- Test logic, keywords, expected payloads.  Those stay in suite files.
- Per-role choices (which firmware to load).  The role bin path is
  selected via `${FW_<ROLE>}` from the resource, but which role to run
  is a suite-level decision.
