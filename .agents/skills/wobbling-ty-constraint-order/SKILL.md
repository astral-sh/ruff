---
name: wobbling-ty-constraint-order
description: Use when a user asks to wobble ty constraint ordering, check constraint-set or TDD ordering determinism, test reversed constraint/typevar IDs, or investigate nondeterministic ty inference and mdtest results.
---

# Wobbling ty constraint order

`TY_CONSTRAINT_SET_ORDER` perturbs both the builder-local TDD-variable order and the local typevar order used to orient typevar-to-typevar constraints. The setting is fixed for the lifetime of each test process.

- unset/`0`: normal ordering;
- `reverse`: reverse both orderings;
- an integer: XOR each local ID with that mask. Small masks immediately perturb dense arena IDs: `1` swaps adjacent IDs, `3` reverses blocks of four, and powers of two exchange neighboring blocks.

This deliberately changes internal TDD shape. Run **mdtests only**: graph-structure unit snapshots are expected to differ. Never enable snapshot updates for a wobble run, since updating would hide the failures being sought.

## Run

From the Ruff root, first establish the normal baseline, then run the reversed and XOR-masked orders sequentially:

```bash
set -uo pipefail
mkdir -p "$HOME/.pi/tmp"
export CARGO_PROFILE_DEV_OPT_LEVEL=1
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
export INSTA_UPDATE=no
export MDTEST_UPDATE_SNAPSHOTS=0
unset INSTA_FORCE_PASS || true

for order in normal reverse 1 2 3 4 7 8 15; do
    if test "$order" = normal; then
        unset TY_CONSTRAINT_SET_ORDER || true
    else
        export TY_CONSTRAINT_SET_ORDER="$order"
    fi

    log="$HOME/.pi/tmp/ty-constraint-order-${order}.log"
    cargo nextest run -p ty_python_semantic --test mdtest \
        --no-fail-fast --status-level fail --failure-output immediate-final \
        >"$log" 2>&1
    status=$?
    printf '%-7s exit=%s  ' "$order" "$status"
    rg 'Summary \[' "$log" | tail -1
    printf '%s\n' "  log: $log"
done
```

Read each failing log and report the mdtest file, section, line, expected result, and actual diagnostic/revealed type. A wobble failure is evidence that inference semantics or displayed solution types still depend on ordering; do not update the mdtest expectations merely to make the wobble run green.

The knob does **not** perturb hashing of Salsa-backed values. The `Solution binding order follows constraint source order` section of `regression/constraint_set_ordering.md` separately varies typevar declaration order to catch binding-order changes caused by draining an `FxHashMap<BoundTypeVarInstance, ...>`.
