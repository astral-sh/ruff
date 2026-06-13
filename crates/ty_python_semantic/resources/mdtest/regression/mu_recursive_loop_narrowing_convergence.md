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
