# Recursive boolean short-circuit convergence

```toml
[environment]
python-version = "3.13"
```

This is minimized from an Apprise ecosystem failure. A loop-carried accumulator used as the left
operand of `and` can contain a cycle-recovery marker. Refining the operand with the short-circuit
truthiness guard used to keep changing the recursive approximation and panic with "too many cycle
iterations".

```py
def f(items):
    success = True

    for item in items:
        result = False
        for value in item:
            result = value
            if result:
                break

        success = success and result

    return success
```
