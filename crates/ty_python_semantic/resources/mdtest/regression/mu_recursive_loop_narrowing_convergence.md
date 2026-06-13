# Recursive loop narrowing convergence

```toml
[environment]
python-version = "3.13"
```

This is minimized from a dd-trace-py ecosystem failure. Applying a truthiness narrowing constraint
to a loop-carried recursive dictionary state used to keep changing the cycle-recovery approximation
and overflow the stack.

```py
def f(items):
    state = {}
    for item in items:
        if item:
            child = state.get(item)  # ty: ignore[possibly-missing-attribute]
            if not child:
                child = {}
                state[item] = child  # ty: ignore[possibly-missing-implicit-call]
            state = child
    if not state.get("end"):  # ty: ignore[possibly-missing-attribute]
        state["end"] = None  # ty: ignore[possibly-missing-implicit-call]
```

This is minimized from a steam.py ecosystem stack overflow. A loop-carried augmented assignment can
feed back into the same variable's equality guard in the rest of the loop body. The loop fixpoint
uses a cycle-recovery approximation for the augmented assignment, and applying the equality
narrowing to that approximation used to re-enter the same predicate.

```py
def decrement_until_zero(condition: bool, limit: int | None = 100) -> None:
    while condition:
        if limit is not None:
            limit -= 1
        if limit == 0:
            return
```
