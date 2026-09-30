## What it does

Detects `while` loops whose condition has unknown initial truthiness but cannot change between
iterations.

## Why is this bad?

If the condition is initially false, the loop never runs. If it is initially true, testing it again
cannot end the loop. This often indicates that the condition should be updated inside the loop.

The rule checks unchanged local variables with immutable values, such as booleans, and identity
comparisons of unchanged local variables. It excludes conditions with calls, attribute lookups, or
potentially stateful truthiness or comparisons, and variables writable from another scope. Explicit
literal conditions such as `while True`, `while 1`, and `while 0` are not reported. Conditions whose
truthiness is statically known are handled by `redundant-condition` and `redundant-condition-strict`
instead.

## Examples

```py
import time


def wait(condition: bool):
    while condition:  # error: [invariant-while-condition]
        time.sleep(0.5)
        print("waiting...")
```

If this behavior is intentional, test the condition once and use an explicit infinite loop:

```py
import time


def wait(condition: bool):
    if condition:
        while True:
            time.sleep(0.5)
            print("waiting...")
```
