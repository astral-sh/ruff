---
name: wobbling-ty-constraint-order
description: >
  Use when a user asks to wobble ty constraint ordering, check constraint-set or TDD ordering
  determinism, test reversed constraint/typevar IDs, or investigate nondeterministic ty inference
  and mdtest results.
compatibility: >
  Requires Cargo, mktemp, and a POSIX-compatible shell; uses cargo-nextest when available and
  otherwise falls back to cargo test.
---

# Wobbling ty constraint order

`TY_CONSTRAINT_SET_ORDER` perturbs both the builder-local TDD-variable order and the local typevar order used to orient typevar-to-typevar constraints. The setting is fixed for the lifetime of each test process.

- unset/`0`: normal ordering;
- `reverse`: reverse both orderings;
- an integer: XOR each local ID with that mask. Small masks immediately perturb dense arena IDs: `1` swaps adjacent IDs, `3` reverses blocks of four, and powers of two exchange neighboring blocks.

This deliberately changes internal TDD shape. Run **mdtests only**: graph-structure unit snapshots are expected to differ. Never enable snapshot updates for a wobble run, since updating would hide the failures being sought.

## Run

From the Ruff root, first establish the normal baseline, then run the reversed and XOR-masked orders sequentially. Set `TY_CONSTRAINT_ORDER_LOG_DIR` to retain logs in a particular writable directory; otherwise `mktemp` chooses an appropriate temporary directory (respecting the environment's temporary-directory configuration).

```bash
set -u

if test -n "${TY_CONSTRAINT_ORDER_LOG_DIR:-}"; then
    log_dir="$TY_CONSTRAINT_ORDER_LOG_DIR"
    mkdir -p "$log_dir"
else
    log_dir="$(mktemp -d -t ty-constraint-order.XXXXXXXX)"
fi
printf '%s\n' "logs: $log_dir"

if cargo nextest --version >/dev/null 2>&1; then
    runner=nextest
else
    runner=test
fi
printf '%s\n' "runner: cargo $runner"

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

    log="$log_dir/ty-constraint-order-${order}.log"
    if test "$runner" = nextest; then
        cargo nextest run -p ty_python_semantic --test mdtest \
            --no-fail-fast --status-level fail --failure-output immediate-final \
            >"$log" 2>&1
    else
        cargo test -p ty_python_semantic --test mdtest >"$log" 2>&1
    fi
    status=$?

    printf '%-7s exit=%s\n' "$order" "$status"
    grep -E 'Summary \[|test result:' "$log" | tail -1 || true
    printf '%s\n' "  log: $log"
done
```

Read each failing log and report the mdtest file, section, line, expected result, and actual diagnostic/revealed type. A wobble failure is evidence that inference semantics or displayed solution types still depend on ordering; do not update the mdtest expectations merely to make the wobble run green.

The knob does **not** perturb hashing of Salsa-backed values. The `Solution binding order follows constraint source order` section of `regression/constraint_set_ordering.md` separately varies typevar declaration order to catch binding-order changes caused by draining an `FxHashMap<BoundTypeVarInstance, ...>`.
